# sv decisions

Distilled Swedish rulings that need more than a `terms.json` line: "X over Y because Z", headings citing the keys they
cover so `pnpm i18n:brief` pulls them into a batch. The term rulings live in `terms.json`, the voice and mechanics in
`style.md`, open questions in `review-queue.md`. Source tiers: macOS (Finder, AppKit, System Settings, read live when
the pile lacks a bundle), then Microsoft terminology, then Total Commander / Nautilus / Thunar / Dolphin, then the
catalog.

## Apple translates Quick Look and Keychain (`commands.fileQuickLook.mac.label`, `menu.file.quickLook`, `ai.secretError.keychainTitle`/`.keychainBody`, `servers.sheet.remember`)

- Quick Look → `Överblick` (Finder `TL14`), so it's not on the don't-translate list, `fileExplorer.quickLookHint.*`
  included.
- The store is `nyckelring` (`Kom ihåg i nyckelringen`), the app is `Nyckelhanterare` (its `CFBundleDisplayName`).
  `servers.sheet.needsStoredSecret` quotes the checkbox label verbatim: change both or neither.

## Apple names are looked up live, never paraphrased (`commands.handler.zoomResetHintMenu`, `main.upgradeNudge.mac`, `errors.listing.ioSerious.suggestion`, `onboarding.stepOptional.*`)

- A string that names a menu item, System Settings pane, or Apple feature spells it as the running macOS does, looked up
  and dated: `Full skivtillgång` (never `fullständig åtkomst till skivan`), `Innehåll > Zoom > 100 %` (never
  `Visa > Zooma`), `Cmdr > Introduktion…`, `Skivverktyg > Skivkontroll`, `Lokalt nätverk` (never the descriptive
  `Lokal nätverksåtkomst`), `Startobjekt och tillägg` (never `Inloggningsobjekt och tillägg`).
- Why: a name that "sounds right" sends the user hunting for a pane that doesn't exist. `{full_disk_access}` in
  `errors.*` resolves from the OS at runtime, so a paraphrase elsewhere shows two names for one thing.

## Git: worktree kept, working tree translated (`errors.git.*`, `settings.fileExplorer.git.showVirtualGitPortal.description`, `fileExplorer.git.size.linkedWorktrees`)

- git's own `worktree` stays English (en-word: `en länkad worktree`, `worktree:n`, plural `worktrees`); the generic
  "working tree" is prose and reads `arbetsträd`; "working directory" is `arbetskatalog`.

## Parent folder and the double-click hint (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`, `fileExplorer.breadcrumb.navigateTooltip`)

- `överordnad mapp` everywhere, as Finder. A list row is a `filrad`; "What just happened?" → `Vad hände nyss?`.

## FAT32 files too large for the drive (`errors.write.filesTooLargeForFilesystem.*`, `fileOperations.errorDialog.tooLargeAndMore`)

- `formaterad med {format}` over `som`, reusing `errors.listing.notSupportedErrno.suggestion`'s phrasing. "and N more"
  is front-loaded `och ytterligare {countText} …` so no trailing word agrees with anything.

## Transfer dialog fields (`fileOperations.transferDialog.*`, `queue.row.label`)

- "will create it during the copy/move" → active `Cmdr skapar den under kopieringen / flytten` over the pile's passive
  `skapas`, with the definite operation noun (`flytten`, never `flyttningen`).
- Queue-row arms are finite present verbs (`Kopierar`, `Byter namn`, `Skapar mapp`), fallback `Arbetar`.

## Archives (`fileExplorer.archiveEnterMenu.*`, `settings.archives.*`, `fileOperations.delete.archiveWarningStrong`)

- `arkiv` is neuter; the bare menu title `Arkiv` (File) never meets the zip sense in one string. A generic bundle is
  `paket`, app bundles `Appaket`, mirroring English's own bundle / app-bundle split.
- Removing from a zip is `ta bort … ur` (out of a container). Settings rows read `Vad Retur gör med en …`.
- The OOXML row (`settings.archives.ooxml.*`) says `Dokument` and bare `paket`: broader than the `Appaket` card below,
  as English's `packages` vs `app bundles`.

## Paste clipboard as a file (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- Active past `Klistrade in … som {filename}` over Nautilus's adjectival `Inklistrad`; `{kind}` arms carry their own
  article, `från urklipp` modifies all three (an `urklipps-` compound doesn't read on all), and the sentence ends on the
  uncontrolled `{filename}`.

## Archive password and compression (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

- `archivePassword.message` agrees with `{name}` (the file: `lösenordsskyddad`, `låsa upp den`); where the sentence says
  `arkivet` the neuter wins (`errors.volume.needsPassword`: `lösenordsskyddat`).
- Slider ends `Snabbare` / `Mindre` name packing speed and output size (Total Commander), not app speed.

## Operation log labels (`operationLog.*`, `commands.logOperationLog.*`)

- Status chips reuse `queue.row.status` word for word. Initiators `Du` / `AI-klient` / `Agent` (the last is a justified
  same-as-source). The skipped chip is `Överhoppad` (review queue).

## Shortcut conflicts and the macOS features they name (`shortcuts.system.*`, `shortcuts.conflict.*`, `downloads.shortcutRow.*`)

- Spotlight, Mission Control, and Spaces stay verbatim. Character Viewer → `Teckenvisare`, Force Quit →
  `Avsluta tvingat`, input source switching → `byte av inmatningskälla`: Apple-style Swedish, not in the pile.
- A modifier key is a `modifierare`: MS's `låstangent` is the lock-key sense. A shortcut's keys are a `kombination`
  (`Välj en annan kombination`); a system-wide one is `global` (`global genväg`, adverb `globalt`).

## Ask Cmdr: chatten, verktygsraderna och kostnaden (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

The read-only AI chat rail: rail UI, tool-call status lines, error copy, chat sessions/search/archive, attachments, the
one-time consent screen, the per-chat cost footer, and the settings section + LLM-call-logging toggle. Reuses
`leverantör` (provider), `modell` (model), `kvot` (quota), `enhet` (drive), `sökväg` (path), `markering` (selection),
`markör` (cursor), `mapp` (folder), `förfrågan` (request), `felsökning` (debugging), `aktivera`/`stäng av`
(enable/disable), `radera`/`ta bort` family, and the "Something went wrong" → `Något gick fel` precedent
(`ai.cloud.genericError` et al.). New terms:

- **chat (a conversation with the assistant): `chatt`** (common gender: en chatt, definite `chatten`, plural `chattar`)
  · MS terminology noun sense (`chatt`), matches everyday Swedish software usage (Messenger/Gmail "Chatt(ar)"). Used for
  `askCmdr.newChat` → "Ny chatt", `threads.open`/`sessions.title` → "Chattar", `sessions.back` → "Tillbaka till
  chatten". `high`.
- **archive a chat (verb, hide from the active list, not delete): `arkivera`**; unarchive → `avarkivera`; archived
  (badge) → `Arkiverad`. MS terminology archive-verb sense (`arkivera`), the mail/chat-app sense, distinct from the
  existing `arkiv` (compressed-file) noun — no collision since the domains never meet in one sentence. `avarkivera` has
  no direct pile hit; composed by the same av-prefix-reversal pattern as `avmontera`/`avinstallera`. `high` for
  arkivera/Arkiverad, `tentative` (composed) for avarkivera.
- **attach (a file/folder to a question, verb) / attachment (noun): `bifoga` / `bilaga`** · MS terminology, both senses
  confirmed (`attach` → `bifoga`, `attachment` → `bilaga`). `askCmdr.attachment.remove` reuses the settled "ta bort"
  (remove from a list/collection) sense: "Ta bort bilaga". `high`.
- **drop (release a drag to attach it): `släpp`** · MS terminology's "Drag and drop" → "Dra och släpp" (ProperNoun);
  `askCmdr.composer.dropHint` "Drop to attach" → "Släpp för att bifoga". `high`.
- **thinking (assistant reasoning before it replies): `Tänker…`** · plain, literal; no jargon needed. `high` (direct,
  unambiguous verb).
- **reply (the assistant's answer, noun): `svar`** (neuter: ett svar, definite `svaret`) · MS terminology (`reply` →
  `svara`/noun sense), matches the app's existing "svara"/"svar" usage. Used as the antecedent for "this one"/"the
  reply" in `askCmdr.error.budgetExhausted` and `unfinishedReply` ("Svaret nådde sin gräns…", "Svaret blev inte klart…")
  rather than a bare pronoun, since English's "this one"/"it" has no single Swedish gender-neutral equivalent standing
  alone. `high`.
- **request (a tool call the assistant asked to make): `förfrågan`** · reused from the termbase entry (API request).
  `askCmdr.tool.refused` "That request wasn't available" → "Den förfrågan var inte tillgänglig". `high`.
- **token (LLM usage unit): `token` / `tokens`** · kept identical to English in both CLDR branches (`sourceHash`
  `askCmdr.cost.tokens` carries `sameAsSourceJustification`). No native Swedish plural is attested in the reference pile
  for this (recent, AI-specific) sense of "token" (the pile's only hit is the older `säkerhetstoken` = security token, a
  different concept); Swedish tech press consistently keeps the bare English plural "tokens" for LLM usage. `tentative`
  (no reference-pile plural; convention from current Swedish tech usage).
- **usage / spending (AI cost tracking): `användning` / `utgifter`** · MS terminology (`usage` → `användning`,
  `spending` → `utgift`, pluralized for the settings section heading). `high`.
- **estimate, adverbial ("about {amount}"): `cirka`** · matches the existing sv catalog's own "cirka"/"ungefär" usage
  for approximate values (`indexing.scan.etaRough` = "ungefär {eta}"). `high`. The onboarding rewrite folded the second
  witness for this into `onboarding.stepAi.local.tooltip`, which renders "about 2 GB" as "runt 2 GB": a third variant,
  fine in that casual register, but don't read it as a reason to change this entry.
- **free (no cost): `gratis`** · matches the shipped `licensing.section.typePersonal` "Personal (free)" → "Personlig
  (gratis)". `askCmdr.cost.free` "free, on-device" → "gratis, på enheten" (on-device processing framed as "på enheten",
  built on the settled `enhet` = device/drive root; no direct pile hit for the Apple-Intelligence-style "on-device"
  phrase, but "på enheten" is the natural, low-risk Swedish rendering). `high` for gratis, `tentative` (composed) for
  "på enheten".
- **dashboard (a provider's billing dashboard): `instrumentpanel`** · MS terminology. `high`.
- **API model call (logged LLM request/response pair): `AI-modellanrop`** · composed on the MS-confirmed "API call" →
  "API-anrop" pattern; `settings.advanced.logLlmCalls.label` "Log AI model calls" → "Logga AI-modellanrop". `high`
  (pattern-confirmed compound).
- **"Not now" (decline button on the consent screen): `Inte nu`** · macOS AppKit (`Not Now` → "Inte nu",
  `en/macOS/AppKit/Document.json`). `high`.
- **talk to (warm framing on the one-time consent screen): `prata med`** · deliberately warmer than `chatta med` (chat
  with) for the one-time opt-in heading, matching the screen's inviting tone; the retired consent heading
  askCmdr.consent.title "Talk to Cmdr about your files" → "Prata med Cmdr om dina filer". `tentative` (stylistic choice,
  no single correct pile rendering for this warmer register).
- **importance (of a folder, the assistant's ranking feature): `vikt`; important → `viktig`** · no reference-pile hit
  (Cmdr-specific ranking feature); composed on the standard adjective/noun pair (`viktig`↔`vikt`), parallel to how
  `askCmdr.tool.importantFolders.*` already uses `viktig`. `tentative` (Cmdr-coined feature; review).
- **Cmdr repeated instead of a bare pronoun, when the sentence names Cmdr's own behavior**: per the established sv
  catalog convention (errors.json etc. always re-use "Cmdr" rather than "den"/"det"), `askCmdr.empty.hint` and
  `ai.cloudConsent.askCmdr.contentsRule` repeat "Cmdr" across sentences rather than introducing an ambiguous pronoun
  (the English `contentsRule` switches to "it" in its second sentence; the Swedish says "Cmdr" all four times). Where
  the antecedent is unambiguous within the same sentence (`settings.askCmdr.intro`'s "Ask Cmdr är skrivskyddad: den
  läser…"), a pronoun is fine.

## Bildindexering på nätverksenheter (`settings.mediaIndex.networkVolumes.*`, `settings.mediaIndex.alwaysIndex*`, `search.imageResults.networkOff`/`.paused`)

Opting a network (SMB) drive into background image-content indexing so its photos become text-searchable, plus an
always-index override for rarely-browsed archives and the honest status lines. Reuses `nätverk` (network), `enhet`
(drive), `indexera`/`indexering` (index), `aktivera`/`stäng av` (enable), `ansluta` (connect), `koppla från`
(disconnect), `pausa`/`pausad` (pause), `Inställningar`, `mapp`, and the shipped `settings.mediaIndex.enabled.*`
phrasing ("Läs texten i dina bilder så att du kan söka i den", "Körs på din Mac"). New/settled terms:

- **photo(s) (the user's photographs being indexed): `bild` / `bilder`** · Apple localizes the Photos app itself to
  "Bilder" in Swedish (pile `sv/macOS`, 6 "Bilder" hits), so "photo" and "image" both render `bild(er)` in Cmdr's
  Swedish. This also keeps the whole feature consistent with the already-shipped card "Bildsökning" and toggle "Indexera
  bildinnehåll". Definite `bilden`/`bilderna`, common gender (en bild). `high`.
- **network drive: `nätverksenhet`** (definite `nätverksenheten`, plural `nätverksenheter`) · compound `nätverk`
  (termbase) + `enhet` (drive); standard Swedish IT compound, matches how the drive surfaces to the user. `high`.
- **reconnect (a drive coming back): `återansluta`** (present `återansluter`) · macOS pile "återansluta" (14 hits);
  åter- + `ansluta` (connect). "resumes when this drive reconnects" → "återupptas när enheten återansluter". `high`.
- **resume (indexing after a pause): `återuppta`** (passive `återupptas` for "it resumes") · reuses the settled queue
  `återuppta` (resume) entry. `high`.
- **disconnected (drive state): `frånkopplad`** · macOS pile "frånkopplad" (6 hits), the state adjective paired with the
  settled `koppla från` (disconnect) verb. "This drive is disconnected" → "Den här enheten är frånkopplad". `high`.
- **gently (reads the network gently, resource-considerate): `skonsamt`** · standard Swedish for sparing/considerate use
  ("skonsam mot"); no direct pile hit, chosen over `varsamt` for the resource-respect sense. `tentative` (convention;
  low risk).
- **photo archive (a rarely-browsed NAS collection, not a zip): `bildarkiv`** · `bild` + `arkiv` (the collection sense
  of archive, distinct from the compressed-file `arkiv` — same word, disambiguated by context). "a photo archive you
  rarely browse" → "ett bildarkiv som du sällan öppnar" (visiting a drive rendered `öppna`, warmer than `bläddra i`
  here). `high`.
- **opt in (turn a drive on for indexing): `välja in` / `aktivera`** · the internal description uses "har valt in för"
  (opted into); the user-facing toggle reuses `aktivera` (enable). `high`.
- **so far / yet (status tail): `hittills` / `än`** · "photos indexed so far" → "bilderna som indexerats hittills"; "Not
  indexed yet" → "Inte indexerad än" (reuses the `finns inte än` precedent). `high`.
- **indexed (ICU plural, `settings.mediaIndex.networkVolumes.indexed`): one → `{countText} bild indexerad`, other →
  `{countText} bilder indexerade`** · common-gender agreement (en bild → `indexerad`), plural adjective `indexerade`.
  Swedish CLDR one/other. `high`.

## Namnbyten i klump, bildindexets omfattning och Ask Cmdrs bildverktyg (`askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`, `fileExplorer.imageIndex.*`, `settings.mediaIndex.scope.*`, `askCmdr.tool.searchPhotos.*`, `askCmdr.tool.imageFacts.*`)

A re-translation review of the 54 keys added for natural-language bulk rename (`askCmdr.renameReview.*`,
`askCmdr.tool.proposeRenamePlan.*`), image-indexing scope (`fileExplorer.imageIndex.*`,
`settings.mediaIndex.scope.*`/`.chosenFolders.*`, `errors.listing.deviceReconnecting.*`,
`fileExplorer.navigation.driveIndex.tooltipCoalesced*`), and the photo tool labels (`askCmdr.tool.searchPhotos.*`,
`askCmdr.tool.imageFacts.*`). Reuses `byt namn`, `mapp`, `fil`, `enhet`, `genomsökning`, `indexering`, `granska`,
`bild`. New/settled terms:

- **rename (the noun, one proposed rename): `namnbyte`** (neuter: ett namnbyte, definite `namnbytet`, plural
  `namnbyten`) · Thunar/Dolphin sv use the noun directly ("Namnbyte", "Avbryt namnbyte", "Namnbyte av flera objekt",
  "Markera enbart filnamnet vid namnbyte"); macOS sv only ever has the verb phrase "Byt namn på …", so the noun comes
  from the file-manager tier. Modal title "Review file renames" → **`Granska namnbyten`**. ❌ NOT `filbyte`, which reads
  as swapping files, not renaming them. `high`.
- **rename plan / rename cycle: `namnbytesplan` / `namnbytescykel`** · compounded on `namnbyte` with the standard `-s-`
  linking element. The `(cycle)` badge stays `(cykel)`: it's the correct Swedish term for a cyclic dependency and the
  tooltip ("Namnbytescykel. Cmdr använder ett tillfälligt namn medan de här filerna roteras.") disambiguates it from the
  bicycle homonym, which is the only real risk. `tentative` for `(cykel)` (no pile hit for either `cykel` or `loop` in
  this sense; review whether a Swedish user reads the bare badge as "bicycle").
- **allow / deny (per-row review buttons): `Tillåt` / `Neka`; allow all / deny all → `Tillåt alla` / `Neka alla`** · MS
  terminology (allow → `tillåta`, deny → `neka`), imperative per the style guide's button rule. `high`.
- **overwrite (as a WARNING BADGE, not an action): `(överskrivning!)`** · the noun, from Total Commander sv
  ("Överskrivning", "Överskrivning av filer", "Överskrivningsalternativ"). The settled action verb stays `skriv över`,
  but an imperative badge beside a blocked row would read as an instruction to overwrite, which is the opposite of what
  the row means. Badges are noun-shaped in sv: `(cykel)`, `(filtillägg)`, `(finns inte)`, `(överskrivning!)`. `high`.
- **file extension (badge + tooltip): `filtillägg`** · macOS Finder sv ("Filtillägg", "Namn och filtillägg", "Om ett
  befintligt filtillägg ska behållas eller skrivas över") and the shipped sv catalog ("Ändra filtillägg?", "Visa
  filtillägg i namnkolumnen"). `filnamnstillägg` is Apple's long form; the short compound is what the catalog already
  uses. `high`.
- **needs attention (blocked row): `behöver ses över`** · "kräver uppmärksamhet" is a literal calque; `se över` is the
  natural Swedish for "give this a look before it proceeds" and matches the modal's `granska` framing. `high`.
- **exclude (a folder from indexing): `utesluta`, NOT `undanta`** · Total Commander sv ("Uteslut", "Vill du utesluta
  sökning i följande kataloger"); in the pile `undantag` only ever means _exception_, never _exclusion_. Aligns the
  status-bar labels with the already-shipped `settings.mediaIndex.excludedFolders.label` = "Uteslutna mappar" and
  `search.systemDirExclude` = "Utesluter vanliga system- och byggmappar". So "Images excluded" → `Bilder uteslutna`,
  "You excluded this folder" → `Du har uteslutit den här mappen`. `high`.
- **lose track of (macOS losing filesystem change events): `tappa koll på`** · the Swedish idiom is `tappa koll på`;
  `tappa bort koll på` is not idiomatic (you can `tappa bort` an object, but you `tappar koll` on a process). No pile
  hit; corrected on grammar. `high`.
- **caches (as a cause of wrong folder sizes): `cachemappar`** · the sv catalog keeps the loanword `cache` only in
  compounds ("resurscache", "Cachetid", "cachas") and never pluralizes it, since sv has no settled plural (`cacher` vs
  `cachar`). "It's usually caches full of small files" means cache DIRECTORIES, so `cachemappar fulla med små filer`
  sidesteps the plural and reads concretely in a sentence about folder sizes. `high`.
- **percent sign: always a space before `%`** · Swedish typography (and the rest of the sv catalog: "Zooma till 100 %",
  "{percentText} %", "Zoom återställd till 100 %."). The space survives interpolation too:
  `fileExplorer.summary.percentSelectedIn` renders `({percent} %) markerat i` where English writes `({percent}%)`, and
  `indexing.progress.percentEta` is `{percent} %, {eta}`: a pure format string still takes the Swedish space. `high`.
- **"Ask Cmdr to prepare it again" → `Be Cmdr att förbereda den igen`** · the EN "Ask" is the sentence-initial
  imperative verb, not the feature name (the feature name would not be capitalized mid-sentence anywhere else in the
  string). Rendering it as "Be Ask Cmdr att…" stacked the verb on the product name. The user is inside the Ask Cmdr
  rail, so the referent is unambiguous. `high`.
- **photo → `bild`, uniformly** · re-confirms the network-drive pass's decision (Apple localizes the Photos app to
  "Bilder"). The four Ask Cmdr tool labels had drifted to `foton`; aligned to `bilder` so the whole photo-indexing
  surface ("Bildsökning", "Bilder indexerade", "Indexera bildinnehåll") reads as one feature. The one legitimate `foton`
  is `onboarding.stepOptional.mtp.desc`, which is about copying photos off a phone with Finder, not the search feature.
  `high`.

For the image-search index status badges (2026-07-22; the 11 `fileExplorer.imageIndex.*` badge/dot tooltips + 2
`settings.mediaIndex.showFileStatusIcons.*` keys). Small status indicators on image files, folders, and drives showing
image-search indexing state. Reuses the settled indexing family; new/confirmed terms:

- **image search (the feature): `bildsökning`** · the catalog's term wherever the feature is named
  (`fileExplorer.imageIndex.file.indexed` = "Indexerad för bildsökning", `ai.cloudConsent.askCmdr.contentsRule` =
  "Bildsökningen fungerar på samma sätt"); definite `bildsökningen`. Compound `bildsökningsstatus` for the drive
  aria-label. `high`.
- **indexed (as a status on a `bild`): `indexerad` / `indexerade`** · en-word agreement with `bild` (termbase index
  family + shipped `settings.mediaIndex.networkVolumes.indexed` "{countText} bild indexerad / bilder indexerade"). The
  standalone file badge takes the en-word `Indexerad` (implied subject `bilden`, en-word), NOT Apple's neuter supine
  `Hämtat` pattern, because Cmdr's badge is always on an image. `high`.
- **waiting to be indexed: `Väntar på att indexeras`** · mirrors Apple Finder's badge AX pattern "Väntar på
  överföring/hämtning/uppdatering" (`macOS/Finder` AXBADGE4/5/6). Passive `indexeras` for the queued state. `high`.
- **re-index: `indexera om` (passive `indexeras om`)** · the `montera om`/`söka igenom på nytt` re-prefix pattern
  (termbase). "Changed since indexing; will be re-indexed" → "Ändrad sedan indexeringen; indexeras om" (`Ändrad` =
  modified, en-word, matches the `Ändrad` column). `high`.
- **couldn''t be indexed (calm failure): `Gick inte att indexera`** · reuses the settled calm-failure form
  `Gick inte att slutföra` (`queue.json`); no bare "fel"/"misslyckades" per style.md. Tight badge tooltip. `high`.
- **excluded from image search: `Ingår inte i bildsökningen`** · `ingå i` = to be included in; definite `bildsökningen`.
  Distinct from the folder-exclusion verb `utesluta` (that's the user action; this is a passive state on one image).
  `high`.
- **status badge (the small overlay marker): `statussymbol` / `symbol`** · the catalog's own precedent for these overlay
  indicators is `symbol` (`settings.listing.sizeMismatchWarning.description` "Visar en varningssymbol på mappar";
  `useAppIconsForDocuments` "appsymboler", "filtypssymboler"). "Show status badges on image files" → "Visa
  statussymboler på bildfiler"; "a small badge" → "en liten symbol". `high` (catalog-internal precedent).
- **image file: `bildfil`** · standard compound bild+fil. `high`.
- **"is off" (a feature disabled for a drive): `är avstängd`** · en-word participle of `stänga av` (termbase
  enable/disable), agreeing with `bildsökning(en)`. `high`.
- **"still working" (indexing in progress, drive dot): `arbetar fortfarande`** · casual/friendly like the EN source;
  implied subject Cmdr. `high` (natural phrasing; no direct pile hit).
- Drive plural strings duplicate the invariant "på den här enheten är" inside both plural branches (the
  `progress.ofTotal` pattern) so the `indexerad`/`indexerade` adjective agrees in number without a second ICU block.

## Bildindexeringens förlopp och inställningar (`settings.mediaIndex.clip.*`, `settings.mediaIndex.progressSummary.title`, `fileExplorer.imageIndex.file.indexing`)

From the image-indexing progress/settings restructure pass (2026-07-23; the 12 keys: 3 card titles, the Semantic search
card's feature label + not-supported/off-but-installed notes + delete-model flow, and the "Indexing now" file badge).
Reuses the settled indexing family (`indexera`/`indexering`, passive `indexeras`), `aktivera` (enable), `modell`
(model), `ladda ner`/`nedladdad` (download/downloaded), `frigöra` (free/reclaim, from `reclaim.freed` "Frigjorde"),
`ta bort` (remove a re-downloadable resource), `mapp`, `bild`, and the calm-failure `Gick inte att…` form. New/settled:

- **search by description (the semantic-photo-search feature, in running copy): `sökning med beskrivning`; toggle label
  "Search photos by description" → `Sök bilder med en beskrivning`** · reuses the shipped `clip.ready` "…sök bland dina
  foton med en beskrivning" pattern and pairs with the card title `Semantisk sökning` (`clip.title`). Generic feature
  noun (no article) as a sentence subject/object: "Sökning med beskrivning kräver…", "…sökning med beskrivning är
  avstängd", "…stänger av sökning med beskrivning". Photos → `bilder` per the settled photo→bild decision. `high`.
- **Apple silicon: kept verbatim `Apple silicon`** · the macOS reference-pile bundle has NO occurrence (pile gap), and
  the English `@key.description` explicitly says "keep it". "en Mac med Apple silicon" mirrors Apple's own "Mac med
  Apple-kisel" structure; the bare English term reads as a recognizable tech proper noun in Swedish. Lowercase `silicon`
  everywhere, as Apple writes it, even where the English source capitalizes it. `tentative` (pile gap; kept per the
  source instruction).
- **enable indexing / folders to index (card titles): `Aktivera indexering` / `Mappar att indexera`** · `aktivera`
  (enable) + `indexering`; `att`+infinitive for "to index". Sentence case. `high`.
- **delete model (reclaim disk): `Radera modell (frigör {size})` / `Radera modellen för semantisk sökning?`** · the same
  `radera` as `ai.local.deleteModel` for the same act (§ Termdriftsgranskning), paired with `Hämta modell`
  (`clip.download`). "reclaim {size}" → "frigör {size}" (verb of `reclaim.freed` "Frigjorde"). Confirm title reuses
  `Semantisk sökning`. `high`.
- **keyword / tag search (in the delete-confirm body): `nyckelordssökning` / `taggsökning`; combined
  `Nyckelords- och taggsökning`** · `nyckelord` (MS keyword) + `sökning`; `tagg` (catalog `Visa taggar`, "macOS
  Finder-taggar") + `sökning`. "keep working" → "fortsätter fungera". `high`.
- **Indexing now (badge tooltip + progress heading, same EN source/sourceHash 44501db): `Indexeras nu`** · passive
  present of `indexera` (implied subject `bilden`/the drive), meaning actively being processed now, distinct from the
  queued `Väntar på att indexeras` (`file.pending`). Serves both `fileExplorer.imageIndex.file.indexing` and
  `settings.mediaIndex.progressSummary.title`. `high`.

## Raderingsdialogens papperskorgsreglage och överföringens Från/Till (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`.targetGroupTitle`)

- **"Move to trash" (switch in the delete dialog, on = papperskorgen, off = permanent delete):
  `Flytta till papperskorgen`** · macOS Finder sv AL13/N153 verbatim; identical to the catalog's
  `transferDialog.titleVerbOnly` `other {Flytta till papperskorgen}` arm, so the switch and the confirm button read as
  one pair. `high`.
- **"Delete" (destructive confirm button while the switch is off): `Radera`** · settled delete verb, identical to
  `transferDialog.titleVerbOnly`'s `delete {Radera}` arm. `high`.
- **"From" / "To" (headings over the source path and over the destination volume + path): `Från` / `Till`** · Total
  Commander sv ships this exact label pair in its copy/move dialog (`662="Från: "`, `663="Till: "`); macOS "Flytta till"
  confirms `till` for a destination. The settled `mål` target noun stays for the destination CONTROLS (`Målvolym`,
  `Målsökväg`); the headings take the light prepositional pair the English uses. `high`.

## Enhetsindexeringens huvudreglage (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`/`.overriddenBadge`, `settings.indexing.enabled.label`, `settings.section.driveIndexing`, `settings.summary.driveIndexing`)

Review of the strings that explain the GLOBAL drive-indexing switch being off. The pass settled the term itself, which
the catalog had been naming two ways.

- **drive indexing: `enhetsindexering`** (index noun `enhetsindex`, definite `enhetsindexet`) · Swedish forms
  `<X> indexing` as a compound, never as `indexering av <X>`: KDE Dolphin sv renders the exactly parallel label "File
  Indexing" → **"Filindexering"** (and "the file indexing service" → "filindexeringstjänsten"); MS terminology has
  `innehållsindexering`, `djupindexeringsjobb`, `indexeringsroll`. The pile has **zero** `indexering av …` phrases. The
  catalog already leaned compound (`Enhetsindexering` onboarding title, `Status för enhetsindexering`, `Enhetsindex(et)`
  in `queryUi`, `externt enhetsindex`, and the sibling section `Bildindexering`). `Indexering av enhet` was also mildly
  ungrammatical: a bare indefinite singular count noun after `av` needs an article or the plural (`av enheten` /
  `av enheter`). So the three phrase-form keys were re-termed to the compound: `settings.indexing.enabled.label`,
  `settings.section.driveIndexing`, `settings.summary.driveIndexing`. `high`.
- **"is off" (a feature switched off): `är avstängd`, not `är av`** · matches the sibling
  `fileExplorer.navigation.driveIndex.tooltipDisabled` ("Indexering är avstängd för den här enheten") and
  `fileExplorer.imageIndex.drive.off`; the participle agrees with the en-word `enhetsindexering`. Turn-on/off verbs stay
  `slå på` / `stänga av` (macOS "Slå på Wi-Fi/AirDrop/fildelning", "Stäng av iCloud"). `high`.
- **"Off with drive indexing" (the overridden-row badge): `Kräver enhetsindexering`** · ❌ NOT
  `Av med enhetsindexering`: `av med` is lexicalized in Swedish as _rid of_ / the exclamative "off with it!"
  (`bli av med`, "av med mössan"), so a grey badge reading `Av med enhetsindexering` parses as a command, exactly the
  imperative-badge trap style.md warns about. The badge sits on a visibly disabled row, so the useful half is the CAUSE;
  `Kräver X` is the catalog's settled pattern for it (`Kräver Apple silicon`, `Kräver en internetanslutning`) and stays
  badge-short (23 chars vs the English 21). `high` for the term, `tentative` for the state→requirement reframing (in
  `review-queue.md`).
- **"stays unindexed" → `indexeras inte`** · chose the attested negation (`Inte indexerad än`, Dolphin "inte indexerad")
  over coining `oindexerad`, which no source has. `high`.
- **"picks up where it left off" → `fortsätter där den slutade`** · natural Swedish; no pile hit for the idiom. `high`.

## Enhetsindex: genomsökningen som letar ändringar (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- **"Checking for changes" (run-kind header) → `Kontroll av ändringar`** · nominal phrase matching the sibling headers
  (`Första fullständiga genomsökningen`, `Snabb uppdatering`); `Kontrollerar` is macOS SV's checking verb (Finder BN9
  "Kontrollerar om innehållet…"), `ändringar` is catalog-settled (`senaste ändringarna`). Chose `kontroll` over the
  colloquial `koll` (which `tooltipCoalesced` uses only inside the idiom `tappade koll på`) · high.
- **"Update the file list" → `Uppdatera fillistan`** · composed from the settled siblings `Spara fillistan` +
  `Uppdatera index` · high.
- **"the check running right now" → `genomsökningen som pågår just nu`** · reuses `genomsökning` as this catalog's
  settled word for a full check (`tooltipCoalesced`: "Cmdrs nästa fullständiga genomsökning") and that string's closing
  `rättar till det` · high.

## Överföringen som står stilla (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

The copy/move dialog and the queue row when a transfer has stopped moving (a parked SMB share or phone). One string,
`.stallNotice`, feeds both surfaces, so it has to fit the narrow row. The notice replaces the ETA line, so it must stay
calm and never reach for `fel`/`misslyckades`.

- **"stalled" / "no progress" → `Inget har hänt på {duration}`** · the pile has NO term for a stalled transfer: macOS
  has no "stalled" string at all, and `förlopp` (macOS "Visa kopieringsförlopp", "stoppa förlopp") is the
  progress-INDICATOR noun, not progress-as-advancement; `framsteg` has zero hits in any sv source (it's the achievement
  sense). So the honest render is the plain-Swedish negated-time clause `Inget har hänt på <tid>` (standard Swedish
  `på` + timespan under negation), which also reads right in the tight queue row where it replaces `{duration} kvar`.
  Matches the catalog's own conversational register (`Vad hände nyss?`). `tentative` (composed, no source term).
- **"Waiting for X to respond" → `Väntar på att {X} ska svara`** · macOS Finder ships this exact construction: `MR3`
  "Waiting for “^0” to accept…" → **"Väntar på att ”^0” ska svara…"** (`sv/macOS/Finder/LocalizableMerged.json`). MS
  terminology confirms respond → `svara`. Prefer it over the shorter `Väntar på svar från …`, which no source has.
  `high`.
- **destination (the thing written TO): `målet`; source (the thing read FROM): `källan`** · MS terminology gives
  destination → `mål` and source → `källa`; Total Commander sv uses both in its copy dialog ("Källa och destination är
  olika!", "målenheten", "målpanelen", `1224`/`2070`/`5328`). Consistent with the settled `mål` target noun (`Målvolym`,
  `Målsökväg`) and the `Från`/`Till` heading pair. `high`.
- **"has stopped moving" → `står stilla`** · no pile source (macOS has no stall wording). Chose the present-state
  `Överföringen står stilla` over `har stannat`, which reads as "has come to a halt / is over" and would overclaim: the
  transfer is still alive, just not advancing. `tentative` (composed).
- **"leave it running in the background" → `låt den fortsätta i bakgrunden`** · reuses the settled `i bakgrunden` (Total
  Commander) and matches the sibling `queueTooltip` ("Håll igång den här i bakgrunden") and `backgroundedToast` ("Körs
  fortfarande i bakgrunden"). `high`.
- **"partly written" → `delvis skriven` / `delvis skrivna`** · macOS Finder `NE111.1`/`NE111.2` "keep a partial copy" →
  **"behålla en delvis kopia"** gives `delvis` in exactly this interrupted-copy context; `skriva` is macOS's write verb
  (`PW18` "Writing track" → "Skriver spår"). The alternative `delvis överförd` (partly transferred) is available if a
  native reviewer prefers the transfer framing over the write framing. `high`.
- **Close (the button that leaves the transfer running): `Stäng`** · macOS AppKit `Close` → "Stäng" (`WindowTabs.json`,
  `Document.json`), verbatim. Distinct from the neighbouring `Avbryt` (Cancel), and it matches the termbase's dismiss
  entry, which reserved `Stäng` for closing a dialog and `avfärda` for dismissing a toast. `high`.
- **"The log has the details." → `Detaljerna finns i loggfilen.`** · same sentence shape the catalog already ships in
  `askCmdr.renameUndo.refusedBatches` ("Detaljerna finns i åtgärdsloggen."). Chose `loggfilen` over a bare `loggen`
  because Cmdr has TWO logs and `åtgärdsloggen` (the undo history) is the wrong one here; `loggfil` is the catalog's
  settled name for the disk log (`settings.logging.openLogFile` "Öppna loggfil"). `high`.
- **ICU shape:** the `stallInFlight` trailing clause is INSIDE both plural branches, unlike English, which leaves it
  outside. Swedish needs it there: the predicative adjective and participle agree with the counted noun
  (`öppen`/`skriven` singular vs `öppna`/`skrivna` plural), so a shared tail would be ungrammatical in one branch.
  Verified with `IntlMessageFormat(msg,'sv')`: 1 → `one`, 2 and 0 → `other`.

## Kopierad sökväg: urklippsbekräftelsen (`fileExplorer.clipboard.copiedPath`)

En nyckel: raden i informationsnotisen efter ⌘⌥C. Sökvägen visas under den, på egen rad med fast teckenbredd, så den är
INTE en platshållare i meningen: meningen slutar med kolon och måste fungera utan den.

- **"Copied the path, it's now on your clipboard:" → `Kopierade sökvägen, den finns nu i urklipp:`** · återanvänder
  `path → sökväg` och `clipboard → urklipp` ur termbasen (macOS Finder) · high. Preteritum först speglar systernotisen
  `Kopierade {countText} objekt`, och `i urklipp` (inte `på urklipp`) är den redan settlade prepositionen
  (`clipboard.empty` = "Inga filer i urklipp."). Inget possessivt "ditt urklipp": det finns bara ett.

## Operation queue: kön byter huvudord (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

The English window was renamed from **"Transfer queue"** to **"Operation queue"**, because it lists deletes, trashes,
renames, folder and file creations, and archive edits too, not only copies and moves; "transfer" also already means
copy-or-move one level down in Cmdr (the transfer progress dialog, the transfer driver). So the source widened from a
narrow word to the CATEGORY word, and Swedish widens the same way. This is a meaning change, not a wording tweak.

- **operation (the category word for a copy, move, delete, trash, rename, create, or archive edit): `åtgärd`** (common
  gender: en åtgärd, definite `åtgärden`, plural `åtgärder`) · already this catalog's settled head noun
  (`operationLog.*` → `Åtgärdslogg`, `settings.navigationAndFileOps.card.operationLog`, `åtgärdshistorik`, the `Åtgärd:`
  field label), and macOS Finder sv confirms it in exactly Cmdr's sense: "Du kan inte byta namn på ”^0” eftersom **en
  annan åtgärd** pågår just nu, t.ex. flytt eller kopiering av ett objekt eller tömning av papperskorgen"
  (`sv/macOS/Finder/LocalizableMerged.json`), plus "Åtgärden kan inte slutföras eftersom …" throughout. MS terminology
  gives operation → `åtgärd` and the compound pattern `operation code` → `åtgärdskod`, `operation type` → `åtgärdstyp`.
  `high`.
- **operation queue (the window): `Åtgärdskö`, definite `åtgärdskön`** · `åtgärd` + linking `-s-` + `kö`, the same
  compound shape as the already-shipped `Åtgärdslogg`, so the two View-menu neighbours read as the deliberate pair the
  English intends: **Åtgärdskö** (what's running now) next to **Åtgärdslogg** (what already ran). `kö` compounds are the
  Swedish norm for this (MS terminology: `målkö`, `leveranskö`, `administrationskö`, `mellanlagringskö`,
  `arbetsuppgiftskö`; Total Commander sv `4005="Kö"`). macOS sv has no queue string at all, so the compound comes from
  the MS + TC tiers on top of the Tier-1 head noun. `high`.
- **Definite vs indefinite, kept apart.** The window TITLE and the command/menu label are indefinite `Åtgärdskö`
  (`queue.windowTitle`, `commands.queueShow.label` — identical to each other per the key description, and matching how
  `Åtgärdslogg` is titled). Running prose takes the definite `åtgärdskön` ("Skicka till åtgärdskön", "hantera den i
  åtgärdskön", "Hitta den i åtgärdskön"). Don't flatten the two into one form. Also note `commands.queueShow.label` lost
  its old "Visa " prefix: English dropped it, and the sibling `commands.logOperationLog.label` is a bare `Åtgärdslogg`,
  so the pair now matches.
- **"Operations" (the heading + the list's aria label): `Åtgärder`** · plural noun, not a verb, per the key description.
  `queue.heading` and `queue.list.aria`. `high`.
- **"this operation" (the per-row aria labels): `den här åtgärden`** · common-gender `den`, definite noun, matching the
  `den här överföringen` shape the rows already used. `high`.
- **ICU count phrase `queuedToastCount`: one → `# åtgärd`, other → `# åtgärder`** · regular Swedish plural on a
  common-gender noun; sv CLDR is one/other. `high`.
- **❌ Not `överföringskö` / `Överföringar`** for the window (an `avoid` in `terms.json`). `överföring` itself stays the
  right word for a copy/move in flight (the transfer progress dialog, `Överföringen står stilla`, `Överföringsmetod`),
  which is exactly the narrower sense English kept one level down.

## Progress chip + failure notice (`queue.row.dismiss*`, `queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`)

Two new surfaces on top of the queue window: a ~80 px progress chip in the main window's top-right corner (an action
word, a bar, a hover tooltip, and a state for operations that stopped early), and a failure notice (a toast that never
auto-dismisses) with a matching failed row carrying a Dismiss button. The head noun and the window's name are settled in
§ Operation queue above; this section adds only what these two surfaces needed.

- **dismiss (button that removes a stopped row / closes a persistent notice): `Avfärda`; "Dismiss all" → `Avfärda alla`;
  the per-row aria → `Avfärda den här åtgärden`** · UPGRADED `tentative` → `high`: macOS AppKit ships the pair directly
  (`sv/macOS/AppKit/TouchBar.json`, `Dismiss Popover` → **"Avfärda popover"**), which outranks MS terminology's
  `dismiss → stäng / stänga av` (`SWEDISH.tbx` entry 780443, defined as turning off a system notification). Keeping
  `Stäng` off this button matters: the catalog reserves `Stäng` for closing a dialog or window (the stalled-transfer
  Close button), and the queue row is a notice, not a window. The catalog already ships `Avfärda` on eight
  toast/notice-clearing controls (`ui.toast.dismissAria` "Avfärda avisering", `crashReporter.dialog.dismiss`,
  `downloads.empty.dismiss`, `errorReporter.sentToast.dismiss`, …), so the row, the toolbar button, and the toast all
  read as one action. ❌ Not `Ta bort`: it's the settled remove-from-a-list verb, but on a row that names a file
  operation it reads as re-deleting the files, which is the one thing Dismiss does not do. `Avfärda alla` is parallel to
  the neighbouring `Pausa alla` / `Återuppta alla`.
- **"Couldn''t finish <action>" (the failure-notice headline family): `Gick inte att slutföra ` + the action as a
  DEFINITE verbal noun** · macOS Finder ships this exact construction: "Det gick inte att slutföra **synkroniseringen**
  av ^0" (`sv/macOS/Finder/LocalizableMerged.json`), and the verb-object collocation "…om du vill **slutföra
  kopieringen**" confirms the copy arm word for word. The nine arms: `kopieringen` / `flytten` / `raderingen` /
  `flytten till papperskorgen` / `namnbytet` / `skapandet av mappen` / `skapandet av filen` / `redigeringen av arkivet`
  / (bare `Gick inte att slutföra`). `high`.
  - The first three nouns are NOT re-derived: `queue.empty.body` in this same file already ships them ("Kopieringar,
    flyttar och raderingar visas här"), so the empty state, the rows, and the toast agree.
  - `skapandet av X` over a compound: Nautilus sv has "skapandet av mappen ”%s”" and "skapandet av katalogen"
    (`sv/gnome-nautilus/nautilus.po`). Same for `redigeringen av arkivet` (`redigera` per the `archive_edit` row arm).
    The `av`-phrase, not a compound (`mappskapandet`, `arkivredigeringen`), because English's definite article points at
    THE specific folder/file/archive this operation touched, not at a feature.
  - **Headline stays CLIPPED (`Gick inte att…`), body copy keeps `Det gick inte att…`.** English's headline drops its
    subject too, the clipped form is byte-identical to the opening of `queue.row.status`'s `failed` arm (so the toast
    and the row visibly say the same thing), and it saves a toast line. The full `Det gick inte att…` stays right for
    running prose (`errors.json` throughout).
  - ❌ Still never `fel` / `misslyckades` on any of these (style.md).
- **`{count} operations couldn''t finish` (summary toast + chip): one → `{countText} åtgärd gick inte att slutföra`,
  other → `{countText} åtgärder gick inte att slutföra`** · count-first forces the noun-first order here, which is fine:
  the house phrase stays intact as the predicate. Regular common-gender plural, sv CLDR one/other. `high`.
- **"Show in operation queue" → `Visa i åtgärdskön`; "Open the operation queue" → `Öppna åtgärdskön`** · DEFINITE, per §
  Operation queue's definite/indefinite split: only the window title and the menu label take the bare `Åtgärdskö`. A
  button label that is a prepositional phrase is running prose. Matches the shipped "Hitta den i åtgärdskön". `high`.
- **"percent" spelled as a word for a screen reader: `procent`** (`queue.chip.ariaLabel`) · Swedish screen readers say
  "procent" for `%` anyway, but spelling it out matches the English source's intent and removes the reader's dependence
  on the symbol. The VISIBLE tooltip keeps the sign, with the mandatory Swedish space: `{percentText} %`. `high`.
- **The chip tooltip's optional clauses each carry their own leading space, and the `=0 {}` / `other {}` arms stay
  empty.** `item(s)` → `objekt` (invariant in both plural branches, per the settled `objekt` entry), `to {destination}`
  → ` till {destination}`. Assembled and read for all four count/destination combinations; no double space, no dangling
  `·`. The trailing `{detail}` is a pass-through of `fileOperations.transferProgress.etaRemaining`, already settled as
  `{duration} kvar` (Nautilus `%T left` → **"%T kvar"**; macOS Finder's alternative is the longer "… återstår" /
  "Beräknar återstående tid…"), or the status word `Pausad`. Nothing to translate in the slot itself; noted so a future
  pass doesn't introduce a second time-left phrasing next to it.
- **Known awkward arm, shared with English: `{label}` + the count clause run together, so the trash arm reads "Flyttar
  till papperskorgen 3 objekt · 42 %".** Swedish would rather say "Flyttar 3 objekt till papperskorgen", but `{label}`
  is opaque, and English accepts the identical wobble ("Moving to trash 3 items"). Deliberately NOT diverged (adding a
  `·` before the count would fix trash and worsen the seven common arms). The fix belongs in the English key's shape,
  not in one locale.

## Den fristående konfliktfrågan (`fileOperations.operationConflict.context`/`.pausedNote`)

The main-window prompt a BACKGROUNDED operation raises on a name clash. The context line sits directly under the
already-shipped title `Filen finns redan`, so it has to read as running text, not as a queue row.

- **The context arms are `queue.row.label`'s verbs plus a destination clause, not a fresh translation.** Swedish's queue
  arms are finite present-tense verbs (`Kopierar`, `Flyttar`), not nominalizations, so they take the clause without
  restructuring, and macOS Finder ships the resulting sentence verbatim: "Kopierar ”^1” till ”^2”" / "Flyttar ”^1” till
  ”^2”" (`sv/macOS/Finder/LocalizableMerged.json`). Preposition `till` is the one already settled in
  `queue.chip.tooltip` (` · till {destination}`). `{destination}` stays UNQUOTED here (English and the chip tooltip are
  both unquoted), even though Finder quotes its own `^2`. `high`.
- **The `other` type arm takes `i`, not `till`: `Arbetar i {destination}`** · the fallback names where work is
  HAPPENING, not where items are going, so the destination is a location, and Swedish marks that with `i`. Using `till`
  there would promise a transfer the operation may not be doing. `high`.
- **`archive_edit` splits from the queue row on purpose: `Redigerar {destination}` (names the archive) vs
  `Redigerar ett arkiv` (no destination to name)** · the queue row's generic `Redigerar arkiv` stays as it is; this key
  needs the indefinite article in its `other` arm because it's a sentence, not a label. `arkiv` is neuter, so `ett`
  (matching this catalog's own "ett arkiv" in `errors`/`fileExplorer`/`operationLog`/`settings`). `high`.
- **"Everything else is paused until you answer." → `Allt annat är pausat tills du svarar.`** · `pausat` is the settled
  queue status word `Pausad` (macOS Finder "Pausad", "Kopiering av ”^0” har pausats") in NEUTER agreement, because the
  subject is `allt annat`; keeping the same root is what makes the note and the queue rows read as one state.
  `tills du svarar` over `tills du har svarat`: the English is a simple present, and the shorter form stays calm under a
  button row. `high` for `pausat`, `tentative` for the `tills du svarar` clause (no pile hit for the idiom; composed).

## Köknappen när kön är tom (`fileOperations.transferProgress.background`/`.backgroundAria`)

The SAME progress-dialog button as `fileOperations.transferProgress.queue`, worded for an EMPTY operation queue: with
nothing to queue behind, English swaps the noun "Queue" for the verb "Background" (an imperative, "put this transfer out
of sight"), not the backdrop noun.

- **"Background" (the button, empty-queue state): `I bakgrunden`** · Total Commander sv ships this exact button:
  `WCMD.LNG.utf8` `{COMMON}` runs `4001="OK"`, `4002="Avbryt"`, `4003="Hjälp"`, **`4004="I &bakgrunden"`**, `4005="Kö"`,
  `4006="&Endast fel"` — the copy-dialog control row, and `4005` is already this catalog's source for `Kö`. So the
  sibling state's Swedish comes from the neighbouring ID in the same dialog of the same orthodox two-pane ancestor.
  Tiers 1 and 2 have nothing to weigh against it: macOS sv has only the backdrop/wallpaper sense ("Bakgrund", "Ändra
  bakgrund…", "bakgrundsfärg") and no run-in-the-background action at all, and Nautilus/Thunar/Dolphin likewise only
  ever mean the view's backdrop ("vyns bakgrund"). MS terminology corroborates the adverbial for the running sense
  (`direktuppspelning i bakgrunden` = background streaming) while its bare `background` entries are all the noun.
  `high`.
  - ❌ Not the bare noun `Bakgrund`: on a button that reads as the backdrop (and it's what macOS's wallpaper strings
    mean), so it lands as a label, not a command. The preposition is what carries the verb sense: `i` + definite forces
    the "where the work goes" reading, exactly the ellipsis English makes ("[run it in the] background").
  - ❌ Not the fuller imperative `Kör i bakgrunden`: correct Swedish, but it's a wordier control than the two-character
    `Kö` it swaps places with, and no source puts a verb on this button. Keep it in reserve if a native reviewer finds
    `I bakgrunden` too elliptical.
- **"Keep this running in the background" (the aria): `Håll igång den här i bakgrunden`** · byte-identical to the
  opening of the shared `queueTooltip` ("Håll igång den här i bakgrunden och hantera den i åtgärdskön (F2)"), so the
  tooltip a sighted user reads and the name a screen reader speaks are the same sentence. Reuses the settled
  `i bakgrunden` (`terms.json` `background`) and matches `queueAria`'s imperative shape ("Skicka till åtgärdskön").
  `high`.
- **WCAG 2.5.3 containment: the label `I bakgrunden` sits inside the aria as `i bakgrunden`**, matching
  case-insensitively (identical letters, only the sentence-case initial differs) — the same bar English keeps
  ("Background" ⊂ "…in the background"). Exact-case containment isn't reachable in natural Swedish here: the aria is a
  clause that starts with its verb (`Håll`), and Swedish capitalizes nothing mid-sentence. See style.md § Notes and
  decisions (the definite-form note) for the trap this avoids: the bare noun `Bakgrund` is NOT a substring of
  `i bakgrunden` (indefinite vs definite), so the noun choice would have broken containment as well as the part of
  speech.

## Quit gate: dialogen som stoppar ⌘Q medan något pågår (`main.quit.*`)

The modal Cmdr raises when the user quits while a copy, move, delete, trash, or archive edit is still running: a
question title, a reassuring body, the list of running operations under a small heading, a live countdown, and the two
buttons. Reuses the settled `åtgärd` head noun (§ Operation queue), `Pågår` (`queue.row.status`), `objekt`,
`delvis skriven` (§ Överföringen som står stilla), and `Avsluta` (the quit verb). New/settled:

- **"Quit while N operation(s) are running?" → `Avsluta medan en åtgärd pågår?` /
  `Avsluta medan {countText} åtgärder pågår?`** · macOS Finder ships the collocation verbatim: "Finder kan inte avslutas
  eftersom **en åtgärd fortfarande pågår** på en iOS-enhet" and "…eftersom några aktiviteter fortfarande pågår"
  (`sv/macOS/Finder/LocalizableMerged.json`), which pins both the quit verb and `åtgärd … pågår` in exactly this
  surface. Total Commander sv ships the same dialog one tier down (`WCMD.LNG.utf8`
  `1237="VARNING: %i pågående aktivitet(er) i bakgrunden!\nAvsluta ändå?"`), confirming the shape; Cmdr keeps its own
  settled head noun `åtgärd` rather than TC's `aktivitet`, so the title, the queue window (`Åtgärdskö`), and the log
  (`Åtgärdslogg`) all say the same word. `high`.
- **"Quitting in N seconds…" → `Cmdr avslutas om {secondsText} sekund(er)…`, NOT a subject-less `Avslutar om …`** ·
  Swedish marks an app quitting ITSELF with the deponent `-s` form, the way Apple does ("Finder kan inte **avslutas**",
  "Finder kommer att **avslutas**", "Systeminställningar **avslutas** och startas om"). Active `Avslutar` is transitive
  (it wants an object) and collides with Finder's own progress stage `Avslutar` = **Finishing** (key `PW21`/`BN4`), so
  it would read as "finishing something", not "quitting". Naming Cmdr as the subject also lets the tail drop the
  English's trailing "on Cmdr" instead of repeating the brand twice in one sentence. `sekund` / `sekunder` per Nautilus
  sv (`msgstr[0] "%d sekund"` / `msgstr[1] "%d sekunder"`). `high`.
- **restart / logout (the OS's, as nouns): `omstart` / `utloggning`** · the verbs are Tier-1 attested as the Apple-menu
  items themselves (`sv/macOS/AppKit/Menus.json`: `Restart` → "Starta om", `Log Out` → "Logga ut"; MS terminology
  agrees, `restart` → "starta om", `sign out`/`log off` → "logga ut"). The nouns are the regular deverbal forms: MS
  terminology has `omstart` directly ("automatisk omstart", "Interaktiv omstart") and `utloggning` in compounds
  ("webbsida för klientutloggning"), and the shipped sv catalog already uses the noun (`settings.json` "Omstart krävs").
  One shared article covers both ("en omstart eller utloggning"). `high`.
- **"never waits on Cmdr" → `aldrig behöver vänta`** · `vänta på` is Finder's own construction (`N178` "…vänta på att
  den visas på skrivbordet"); with `Cmdr` already the sentence subject the object is implicit, so the shorter clause
  reads better than repeating the brand. `high`.
- **"Whatever''s finished stays done." → `Allt som redan är klart förblir klart.`** · `klar` is the catalog's settled
  done-state word (`queue.row.status` `done {Klar}`), neuter agreement with `allt`. `high`.
- **"anything still being written" → `Allt som fortfarande skrivs`** · **the body must stay number-neutral**: one
  operation writes several files at once and several operations can run at once, so
  `det enda objektet som fortfarande skrivs` states something false. `Allt som` scopes it without a numeral and mirrors
  the opening `Allt som redan är klart`; `skrivas` is macOS's write verb (`PW18` "Writing track" → "Skriver spår").
  `high`.
- **"what it leaves half-written" → `det som blivit delvis skrivet`** · identical concept to the already-settled "partly
  written" (§ Överföringen som står stilla, `transferProgress.stallInFlight` "kan redan vara delvis skriven/skrivna"),
  so it reuses that wording rather than coining `halvskriven`; neuter agreement with `det`, since the definite
  `den delvis skrivna filen` can't stay number-neutral. `high`.
- **"clears away" (the cleanup, softer than deleting) → `rensar bort`** · the catalog's own soft-removal verb
  (`errorReporter.dialog.description` "…rensas bort innan de skickas"); deliberately NOT `raderar`, which is the settled
  destructive delete the user asked for elsewhere, nor `tar bort`. `high`.
- **"Still running" (heading over the operation rows) → `Pågår fortfarande`** · reuses `queue.row.status`'s running arm
  `Pågår` plus Finder's own `fortfarande pågår` adverb placement, so the heading and the rows below it say the same
  word. Finite verb with the list as its implied subject, mirroring English's participle. `high`.
- **"Keep working" (the button that calls the quit OFF) → `Fortsätt arbeta`** · `Fortsätt` is macOS's Continue
  (`sv/macOS/AppKit/NSExceptionAlert.json` `66.title`, Finder "Klicka på Fortsätt om du vill…"). It is an imperative to
  the USER, so it can't be misread as postponing (no `senare`, no `påminn mig`) and can't be misread as cancelling the
  operations (that button is `Avbryt` and isn't on this dialog). `high`.
- **"Quit now" → `Avsluta nu`** · the settled quit verb (`commands.appQuit.label` "Avsluta Cmdr", macOS "Avsluta
  Finder") plus `nu`, which is load-bearing: the app quits either way when the countdown ends. ❌ Deliberately NOT
  macOS/TC's `Avsluta ändå` (Quit anyway): that answers "should I at all?", while this button answers "skip the wait".
  `high`.
- **`countdownAria` → `Tid kvar tills Cmdr avslutas av sig själv`** · not a Label-in-Name pair (the countdown region has
  no visible label to contain), so it just names what the number measures; `kvar` is the catalog's settled
  time-remaining word (`etaRemaining` "{duration} kvar"). `high`.

## Användningsstatistik utan "anonym", med "ett slumpmässigt id" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `pseudonym` / `pseudonymiserad` —
that jargon is exactly what the copy avoids.

- **usage stats → `användningsstatistik`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the
  `anonym` adjective was cut. MS terminology's `användningsdata` is the data sense; the statistics reading is what the
  UI says · high
- **a random id → `ett slumpmässigt id`** · MS terminology (random → `slumpmässig`) · high. ❌ Not `identifierare` (MS
  for "identifier"): clunky and technical; `id` is what a Mac user reads daily (Apple-ID).
- **tied to → `kopplad till`** · the catalog's own verb (`onboarding.stepBeta.emailNote` "kopplas aldrig till din
  användningsstatistik") · high
- `analyticsLede` drops the comma before `och` in "Det är på nu och du kan stänga av det när som helst" per the style
  guide's short-clause rule.

## Frågan som stoppar en kö-rad och ångradialogen (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- **"Needs your answer" (queue-row status) → `Behöver ditt svar`** · ⚠️ NOT `Väntar på svar`: the same narrow column
  shows `Väntar` for "queued behind another operation", so a status opening on that word is unreadable at a glance.
  `behöver` is macOS-attested ("Du behöver en administratörs användarnamn"), and the answering verb matches
  `fileOperations.operationConflict.pausedNote` ("tills du svarar") · high
- **the prompt (the on-screen question) → `frågan`** · same framing as `pausedNote`; macOS Finder carries the question
  sense in "Fråga inte igen" (`PE122`) · high
- **"carries on" → `så fortsätter …`** · `fortsätta` is the catalog's continue verb (`main.quit.keepWorking` = "Fortsätt
  arbeta"). Keep the comma before consequence-`så`; style.md's no-comma rule covers `och`/`eller`, not `så` · high
- **"Keep them" (the safe button) → `Behåll dem`** · macOS AppKit `Keep` → `Behåll`, `Keep Both Files` →
  `Behåll båda filerna` · high
- **"Roll back" / "Roll this operation back?" → `Ångra` / `Ångra den här åtgärden?`** · the settled `ångra` rollback
  family (§ Rollback-familjen, matching `transferProgress.conflictRollback`); the bare-infinitive question mirrors
  `main.quit.title` ("Avsluta medan en åtgärd pågår?") · high
- **"Stop" in the rollback tooltip → `Stoppa`** · macOS Finder "Stoppa" (`PE107`, `SD23`, "stoppa processen och behålla
  en delvis kopia"). ❌ Never `Avbryt` here: that IS the Cancel button, which KEEPS the finished files, and the tooltip
  exists to say rollback doesn't · high
- **"so far" → `hittills`** · standard Swedish, no direct pile hit; unambiguous · tentative (convention)
- **the files an operation overwrote → `filer som den har skrivit över`** · the settled `skriv över` (style.md).
  English's "replaced" is the overwrite sense here, so don't reach for `ersätta` · high
- `foregroundBusyToast` no longer claims an operation is in the way ("Något annat är öppet här"): the blocker can be any
  dialog. "bring this one up" → `ta sedan fram den här` (`åtgärd` is common gender, so `den`) · high

## Rollback-familjen: `ångra`, inte `återställ` (`fileOperations.transferProgress.*`, `operationLog.*`, `commands.logOperationLog.*`)

Rollback heter `ångra` i hela katalogen, samma ord som `settings.operationLog.intro` (`ångra åtgärder`).

- **rollback → `ångra`** · macOS `sv` (`Undo` → "Ångra", "Du kan inte ångra det här kommandot."), Nautilus `sv` ("_Ångra
  Kopiera", "_Ångra Flytta" — exakt vår domän: en filhanterare som ångrar en filåtgärd), Microsoft `sv` (`undo` →
  "ångra") · high.
- ❌ Inte `återställ`: det ÄR `restore` i svenskan (macOS och Microsoft `sv` `restore` → "återställa"), och rollback
  återställer just inte — den raderar det åtgärden skrev, och en fil som skrevs över är borta (det säger
  `rollbackConfirm.body` rakt ut). Microsoft `sv` ger visserligen `roll back → återställa`, men det är
  databastransaktionens betydelse, där det tidigare tillståndet verkligen kommer tillbaka: sense-fällan nr 4 i
  `docs/i18n/reference-pile/how-to-mine.md`. Katalogen använder dessutom `återställa` för äkta återställning
  (`askCmdr.renameUndo.*`, där de gamla namnen faktiskt kommer tillbaka, och `reset to default`).
- Ingen krock med Cancel: den heter `Avbryt` / `Avbruten`, så `Ångra` / `Ångrad` står fritt och de två statusarna går
  att skilja åt i samma kolumn.
- De sex pastillerna: `Går inte att ångra` / `Går att ångra` / `Ångrar` / `Ångrad` / `Delvis ångrad`, och
  `operationLog.outcome.rolledBack` återanvänder `Ångrad` (engelskan använder samma sträng på båda ställena).
- `transferProgress.smbNativeNote` skrevs om till verbform ("Det kan ta tid att avbryta eller ångra") i stället för
  substantivet `ångring`, som är korrekt men styltigt i ett gränssnitt.
- Oförändrade för att de redan stämmer: `rollbackConfirm.body`, `rollbackConfirm.keep`,
  `transferProgress.rollbackTooltip` (`Stoppa`-posten ovan gäller fortfarande).

## Kedjade namnbyten: toasten som räknar de övriga (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

Samma toast som `fileExplorer.rename.chainKeptOriginalName`, omskriven varje gång ytterligare en fil i pilkedjan
behåller sitt namn: den namnger den senaste och räknar de tidigare. Systersträngens `”{name}” behåller sitt namn` är
redan satt, så den här nyckeln får bara ett påhäng, inte en ny formulering.

- **"kept its name" → `behåller sitt namn`** (oförändrat från systersträngen) · Total Commander sv sätter kollokationen
  i precis den här domänen: `WCMD.LNG.utf8` `1673="&Behåll namnet;Avbryt"` och `1674="&Behåll namnet;Behåll &alla;…"` är
  knapparna i namnkonflikten. macOS sv har verbet i samma sammanhang ("Om befintliga objekt med samma namn i målmappen
  ska **behållas** eller skrivas över", `Finder/LocalizableMerged.json`) men ingen färdig mening att kopiera. Presens,
  inte preteritum: engelskans "kept" ser tillbaka på en åtgärd som just misslyckades, medan svenskan här beskriver
  tillståndet filen står i. `high`.
- **"N other files" → `{othersText} andra filer`, singular `en annan fil`** · Nautilus sv översätter
  `%'d other item selected` / `%'d other items selected` → **"%'d annat objekt markerat"** / **"%'d andra objekt
  markerade"**, alltså exakt det räknade "other" vi behöver, i filhanterardomänen. Vi byter objekt mot `fil` eftersom
  nyckeln bara gäller filer; `fil` är utrum, så det blir `en annan fil` / `andra filer`. `high`.
- **"and so did …" → `, liksom …`** · svenskans täta motsvarighet till engelskans pro-verb: bisatsens verb elideras, så
  det reflexiva `sitt namn` aldrig behöver böjas om till `sina namn`. Ingen träff i högen (macOS sv använder i stället
  `och ytterligare ^0.` för "and ^0 more.", `N141.3`), men `liksom` är standardsvenska och håller meningen i ETT stycke,
  vilket toasten behöver. ❌ Inte `och det gör …`: `och` mellan två korta huvudsatser tar ingen kommatecken enligt
  style.md, och "behåller sitt namn och det gör tre andra filer" blir oläsbart utan pausen. ❌ Inte
  `och detsamma gäller …`: korrekt men byråkratiskt, tvärtemot husrösten. `tentative` (sammansatt; låg risk).
- **Citattecknen är `”…”`** i båda systersträngarna, per style.md; macOS sv skriver sitt eget namn-slot likadant
  (`N141.2` = `\n\t”^0”`).
- `{reason}` är text Cmdr inte helt styr över och avslutas utan punkt, så meningen börjar med en inskjuten främmande
  sats. Om en `reason` någon gång slutar med `?` eller `!` blir `. ` efter den fel i båda språken; då är det engelskans
  nyckelform som ska ändras, inte den här översättningen.

## Obekräftade namnbyten och det oanvändbara namnet (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

Systerparet till `chainKeptOriginalName*`, men med motsatt innebörd: där säger vi att filen definitivt behåller sitt
namn, här säger vi att vi inte vet, och att namnbytet mycket väl kan ha gått igenom. Formuleringarna får aldrig glida
ihop.

- **"Couldn't confirm the rename of X" → `Det gick inte att bekräfta namnbytet av ”X”`** · katalogens egna
  systersträngar sätter mallen för hela den här familjen: `fileOperations.mkdir.timeoutMessage` ("Det gick inte att
  bekräfta att mappen skapades. Volymen kan vara långsam, så mappen kan ändå ha skapats.") och
  `fileExplorer.pane.trashUnconfirmedToast`. macOS `sv` bekräftar `bekräfta` för `confirm` och `Det gick inte att …` som
  huvudmall för en åtgärd som inte gick vägen ("Det gick inte att byta namn på bilden ”%1$@” till ”%2$@”.",
  `Finder`/`AppKit`). Substantivet `namnbyte` styr `av`, inte `på`: Thunar/Dolphin `sv` skriver "Namnbyte av flera
  objekt", "namnbyte av en fil", "namnbyte av flera filer". `high`.
- **Flera på en gång → `namnbytena av ”X” och …`** (bestämd plural) · engelskan behåller singular "the rename of X and N
  other files" fastän det handlar om flera; svenskan blir tydligare i plural, och `en annan fil`-grenen ger ändå två
  namnbyten. Räkneleden `en annan fil` / `{othersText} andra filer` är oförändrad från `chainKeptOriginalNameAndOthers`
  (Nautilus `sv`, "%'d annat objekt markerat" / "%'d andra objekt markerade"). `high`.
- **"it may have gone through anyway" → `så filen kan ändå ha bytt namn`** (plural: `så filerna kan ändå ha bytt namn`)
  · exakt formen `mkdir.timeoutMessage` redan använder: subjekt + `kan ändå ha` + supinum av själva åtgärden. ❌ Inte
  `så det kan ändå ha gått igenom`: `gå igenom` i betydelsen "lyckas" finns inte belagd i högen, och de enda träffarna
  är den andra betydelsen ("Går igenom alla visningslägen", Nautilus `sv`) — sense-fällan i
  `docs/i18n/reference-pile/how-to-mine.md`. ❌ Inte `så det kan ändå ha lyckats`: `lyckas`/`misslyckas` finns inte
  någonstans i sv-katalogen, husrösten undviker medvetet den statusetiketten. `high`.
- **"The volume may be slow" → `Volymen kan vara långsam`** · ordagrant syskonsträngarnas hedge (`mkdir.timeoutMessage`,
  `trashUnconfirmedToast`), som engelskan i den här nyckeln numera också använder. Vi vet inte att volymen är långsam,
  vi gissar, och `kan vara` bär gissningen. `volym` är den satta termen (style.md). `high`.
- **"That folder/filename can't be used" → `Det här mappnamnet/filnamnet kan inte användas`** · macOS `sv` har frasen
  ordagrant i vår domän: "Namnet ”^0” kan inte användas.", "Namnet ”^0” kan inte användas eftersom det är för långt."
  (`Finder`). Behåller engelskans deixis (`Det här …`), eftersom nyckeln också skjuts in i en längre mening om filen som
  behåller sitt namn. Ingen avslutande punkt, per nyckelns kontrakt. Syskonens `Mappnamn får inte …` /
  `Filnamnet är för långt` är kvar som de är: `får inte` är regeln användaren bröt, `kan inte användas` är
  samlingsfallet där filsystemet inte säger vilken regel det var. `high`.

## Föreslagna åtgärder: rutan för det Ask Cmdr föreslår (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- ops (agentens föreslagna filåtgärder) → `åtgärder`; titeln blir `Föreslagna åtgärder` · följer husets "File
  operations" → `Filåtgärder` · high
- approve → `Godkänn` · standard; valt framför macOS `Ta emot`, som hör till att ta emot en AirDrop-fil och inte till
  att låta något köra · high
- reject → `Avböj` · macOS Finder, paret Ta emot/Avböj i AirDrop-rutan (Tier 1) · high
- "This can't be undone" → `Det här går inte att ångra` · macOS Finder ("Den här åtgärden går inte att ångra"),
  förkortat till en etikettrad · high
- "Ask Cmdr's reason" → `Ask Cmdrs skäl` · genitiv på varumärket, enligt regeln om att märkesnamn får böjas · high

## Duplicera: kommandot som kopierar i samma mapp (`commands.fileDuplicate.*`)

- **duplicate (kommandot som kopierar markeringen i dess egen mapp) → `Duplicera`** · macOS Finder `sv`, menyn "Arkiv >
  Duplicera" (`N154`), plus "Duplicera objekt" och "Duplicerar objekt där de befinner sig" (verifierat på macOS 26.6.1,
  `Finder.app/Contents/Resources/sv.lproj`, 2026-08-19) · `high`. Krockar inte med `Kopiera` (F5) eller `Flytta` (F6).
- **"Make a copy of the selected files in the same folder" → `Skapa en kopia av de markerade filerna i samma mapp`** ·
  imperativ, som systerbeskrivningarna ("Kopiera markerade filer…"); `markerade filer` är katalogens term för selected
  files, och "samma mapp" är den mapp filerna redan ligger i · `high`.

## Inbyggda menyer: menyrad, snabbmenyer, fönstertitlar (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Källor för hela gruppen: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/sv.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) är Tier 1 och avgör nästan allt; den engelska sidan läses i `en_GB.lproj`, eftersom
`Base.lproj` bara innehåller kompilerade nib-filer. Safari 26 (`MainMenu.strings`) ger flikorden, Microsofts terminologi
det Apple inte namnger. RAW-familj: **enkla apostrofer**, ett `''` skulle synas dubbelt i menyn.

- **View-menyn → `Innehåll`** · macOS Finder (`206.title`) OCH Safari (`200.title`) `sv` · high. Överraskande men
  konsekvent: Apples svenska View-meny heter `Innehåll`, inte `Visa`. Två Tier-1-appar säger samma sak, så det är Apples
  standard och inte en Finder-egenhet.
- **Övriga menyrubriker → `Arkiv`, `Redigera`, `Gå`, `Fönster`, `Hjälp`, `Tjänster`** · macOS Finder och Safari `sv` ·
  high.
- **Select-menyn (filmarkering) → `Markera`** · macOS Finder (`Markera allt`) och Dolphin `sv` · high. `Markera` är
  ordet för att markera objekt; `Välj` reserveras för att välja ett alternativ.
- **Quick Look → `Överblick`** · macOS Finder (`TL14`) · high. Apple översätter funktionsnamnet, därför står det INTE på
  don't-translate-listan.
- **Get Info → `Visa info`, Enclosing Folder → `Överordnad mapp`, Go > Home → `Hem`, Sort By → `Sortera efter`, Date
  Created → `Skapelsedatum`, Default → `Förval`, Other… → `Annan…`, Hide Others → `Göm övriga`** · macOS Finder Tier 1 ·
  high.
- **Window > Zoom → `Zooma` (verb)** vs **textzoom-undermenyn → `Zoom` (substantiv)** · macOS Finder (`300667.title`) ·
  high. Engelskan säger `Zoom` båda gångerna; svenskan skiljer dem åt.
- **ascending / descending → `Stigande` / `Fallande`** · Thunar + Dolphin `sv` · high.
- **changelog → `Ändringslogg`** · Microsofts terminologi · high. Skilt från Hjälp > `Nyheter`: det ena namnger
  dokumentet, det andra nyheten.
- **word wrap → `Automatiskt radbyte`** · Microsofts terminologi · high.
- **pin / unpin tab → `Fäst flik` / `Lossa flik`** · Microsofts terminologi (`fästa`) plus katalogens
  `commands.tabTogglePin.label` (`Växla fäst flik`) · high. Safari `sv` säger `Nåla fast flik`; `fäst` väljs för att
  motsatsen (`lossa`) blir naturlig och för att katalogen redan använder den stammen.
- **„Edit in editor” → `Öppna i redigeraren`** · beskrivande · tentative. Den ordagranna `Redigera i redigeraren`
  upprepar samma stam; `öppna i` läser naturligt och skiljer sig från `Visa` raden ovanför.
- **Finder-etikettfärger → `Röd, Orange, Gul, Grön, Blå, Lila, Grå`** · macOS Finder (`TG_COLOR_*`) · high.
- **Taggraden (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`) → `Taggar`, `Lägg till ”{color}”`,
  `Ta bort ”{color}”`** · macOS Finder (`TG5`, `TG6`, `N169.37`) · high. Samma verbpar som
  `commands.tagsToggleRed.description` (”Lägger till eller tar bort”).
- **busy (volym som används) → `(upptagen)`** · Microsofts terminologi · high.
- **Eject → `Mata ut`, Disconnect → `Koppla från`, Remove (ur en lista) → `Ta bort`** · macOS Finder · high. `Radera` är
  fortfarande reserverat för permanent radering, enligt `style.md`.
- **Avsiktligt identiska med engelskan** (med `sameAsSourceJustification`): `menu.view.zoom`, `menu.tag.orange`,
  `menu.view.askCmdr`.

## Aviseringen när Cmdr fastnade på systemanslutningen (`fileExplorer.network.osMountFallback.*`)

Tre nycklar: brödtexten i aviseringen som visas när Cmdrs egen, snabbare SMB-anslutning inte gick att öppna, knappen som
gör ett nytt försök, och krysstipset. Delningen fungerar; det enda som är sämre är farten, så tonen är lugn och
förklarande, aldrig varnande.

- **native (macOS egen SMB-väg) → `inbyggd`** · macOS `sv` (`Inbyggd skärm`, `Inbyggd Retina-skärm`, `Inbyggd kamera`,
  verifierat i piles `sv/macOS/`, 2026-08-21) och katalogens `settings.mediaIndex.privacyNote` ("Apples inbyggda
  Vision-ramverk") · `high`. ❌ Inte Microsofts `ursprunglig` (`SWEDISH.tbx`, term 83512): den termen betyder
  original-/ursprungsformat, inte "det som operativsystemet själv tillhandahåller". Samma väg heter `systemanslutning` i
  `navigation.connectionTooltipSystem` och `fileOperations.transferDialog.smbNativeNote`; här nämns `macOS`
  uttryckligen, precis som i engelskan, så `inbyggd` bär beskrivningen och `systemanslutning` namnet.
- **Genitiv på `macOS` → bar form före substantivet (`macOS inbyggda SMB-anslutning`)** · katalogens etablerade mönster
  (`macOS textstorlek`, `macOS säkerhetspolicyer`) · `high`. Namn som slutar på s-ljud tar varken `-s` eller apostrof på
  svenska. `Cmdr` slutar på konsonant och tar däremot vanlig genitiv: `Cmdrs direktanslutning` (samma som katalogens
  `Cmdrs nästa fullständiga genomsökning`).
- **Multiplikator `4x` / `100x` → `fyra gånger` / `100 gånger`** · katalogens `network.reconnect.twice` ("två gånger")
  och `driveIndex.tooltipCoalesced` ("{countText} gånger") · `high`. Svenskan skriver multiplikatorn med `gånger`, och
  siffergränsen (ett till nio med bokstäver, 10+ med siffror) gäller som vanligt, därför `fyra` men `100`.
- **"Try connecting directly" (knappen) → `Försök ansluta direkt`** · imperativ enligt `style.md`, och samma ordval som
  `navigation.connectDirectly` ("Anslut direkt för snabbare åtkomst") och `pane.connectedDirectlyToast` · `high`.
  `direkt` läses här som direktanslutning tack vare brödtexten ovanför, inte som "omedelbart".
- **"Dismiss" (krysset) → `Avfärda`** · termbasens dismiss-post (macOS AppKit `Avfärda popover`) och systernyckeln
  `lowDiskSpace.toast.closeTooltip`, som redan säger `Avfärda` · `high`. `Stäng` är fortsatt reserverat för dialoger och
  fönster.
- **Utelämnat `network` i "native SMB network connection"** · `SMB-nätverksanslutning` blir ett tungt trippelkompositum
  utan att tillföra något: SMB ÄR nätverk, och aviseringen visas i nätverksvyn. `macOS inbyggda SMB-anslutning` säger
  samma sak och läses som svenska.

## Namnbyten och volymsvar som inte gick igenom (`errors.mutation.*`, `errors.volume.*`)

31 nycklar: enradsmeddelandet under namnfältet (eller i en kort avisering) när ett namnbyte, en ny mapp eller en ny fil
inte gick igenom. Familjen är RAW, inte ICU, så `{path}` står som en bokstavlig token och apostrofer är vanliga. Tonen
är den redan etablerade i `errors.write.*`: lugn, aktiv, ingen skuld på personen, inget "fel" som etikett för händelsen.

- **rotmapp (en volyms översta mapp): `rotmapp`** · Xfce Thunar `sv` har exakt begreppet ("The root folder has no
  parent" → "Rotmappen har ingen förälder") och Total Commander `sv` använder det genomgående ("Gå till rotmappen",
  "ZIP-filens rotmapp") · `high`. macOS har ingen egen term för en volyms rot ("Top Level Navigator" → "Navigerare på
  övre nivå" gäller datorns toppnivå, inte en volyms), så tvåpanelslinjen får avgöra.
- **System Integrity Protection: behålls oöversatt** · Apple själv låter namnet stå kvar på svenska (`sv/macOS/Finder`
  nyckel `ET6`: "… kan inte raderas på grund av System Integrity Protection", verifierat i pilen 2026-08-23) · `high`.
  Den står inte i `BRAND_WORDS`, men Apples egen svenska är belägget.
- **Get Info-fönstret: `Finders fönster Visa info`** · termbasens `Visa info` + macOS egen konstruktion ("Visar fönstret
  Visa info för ett eller flera objekt", `sv/macOS/Finder/Localizable`) · `high`. `Finder` slutar på konsonant och tar
  vanlig genitiv (`Finders`), enligt `style.md`.
- **locked / unlock: `låst` / `lås upp`** · macOS `sv` genomgående ("Objektet ”^0” är låst", kryssrutan "Låst", knappen
  "Lås upp") och katalogens `errors.write.fileLocked.suggestion.mac` · `high`.
- **lost track of: `tappade koll på`** · katalogens egen etablerade vändning
  (`fileExplorer.navigation.driveIndex.tooltipCoalesced`: "macOS tappade koll på ändringar i filsystemet") · `high`.
  Pilen saknar frasen, så katalogen är källan.
- **went through / land (att ändringen faktiskt utfördes): `gick igenom` / `gå igenom`** · katalogens
  `errors.listing.deviceReconnecting.explanation` ("så den här åtgärden gick inte igenom") · `high`. Används både för
  `deviceDisconnected` (gick inte igenom) och för `timedOut` (kan fortfarande gå igenom).
- **didn't answer in time: `svarade inte i tid`** · macOS `sv` ("slutfördes inte i tid", "avslutades inte i tid") plus
  Microsofts `tidsgräns` · `high`. Tidsgränsnycklarna säger alltså inte "tidsgränsen nåddes" här: `svarade inte i tid`
  är kortare och namnger vem som teg.
- **lösenordsskyddad om ett arkiv: neutrum `lösenordsskyddat`** · `arkiv` är ett neutrumord (ett arkiv, arkivet), så
  `errors.volume.needsPassword` blir "Det här arkivet är lösenordsskyddat" · `high`. Den tidigare noteringen om
  utrumform i § The archive password and compression gäller `fileOperations.archivePassword.message`, där adjektivet
  kongruerar med `{name}` (filen), inte med ordet `arkiv`.
- **stöder inte / kunde inte slutföra: `den åtgärden`** · `errors.volume.notSupported` och `.ioError` säger bara "that"
  på engelska; svenskan behöver ett huvudord, och beskrivningen namnger den begärda åtgärden. `åtgärd` enligt termbasens
  operation-post och macOS ("Åtgärden kan inte slutföras eftersom den inte stöds") · `high`.
- **Ingen skuld på personen i namnbytesspärrarna** · "Renaming can't take an item out of an archive" blir
  `Ett namnbyte kan inte flytta ut ett objekt ur ett arkiv`, inte "Du kan inte …". `ur` för ut-ur-behållare enligt
  termbasens arkivpost ("ta bort … ur zip-arkivet") · `high`.
- **Citattecken runt `{path}`: `”…”`** · engelskan använder raka `"` här, men svenska katalogen har `”…”` genomgående (§
  Cross-file consistency reconciliation) · `high`. `{path}` är okontrollerad text, så meningarna slutar på den där det
  går (`Det finns inte längre något på ”{path}”.`).

## Papperskorgen i namnbytesspärrarna (`errors.mutation.trashNotSupported`/`.trashRefused`)

Två nycklar till i `errors.mutation.*`-familjen (RAW, ingen ICU), samma enradsyta under namnfältet.

- **"has no Trash" → obestämd form `har ingen papperskorg`** · termbasens arkivpost säger redan "Det finns ingen
  papperskorg i ett arkiv." för samma sorts påstående, och `style.md` sätter `papperskorgen` som termen · `high`.
  Bestämd form bär namnet på funktionen, obestämd bär "det finns ingen sådan här".
- **"the only way is to delete permanently" → `så det går bara att radera permanent`** · `radera` är den satta termen
  för permanent radering (`style.md`) och katalogens `errors.write.trashNotSupported.suggestion` säger redan "radera
  permanent" · `high`.
- **"macOS wouldn't move this to the Trash." → `macOS nekade flytten till papperskorgen.`** · macOS Finder `sv` har
  exakt mönstret "namngiven part nekar" (`”^0” nekade din begäran.`, verifierat i pilen 2026-08-23), och
  `flytten till papperskorgen` är katalogens egen vändning (`errors.write.cancelled.message.trash`) · `high`. Engelskan
  är avsiktligt kort eftersom den tekniska orsaken visas separat, så svenskan lägger inte till något skäl. ❌ Inte det
  opersonliga `Det gick inte att flytta till papperskorgen` som `errors.write.*`-rubrikerna använder: här namnger
  engelskan `macOS` som den som sa nej.

## Kraschdialogens tre varianter: kraschade, fortsatte, eller vet inte (`crashReporter.dialog.body.keptRunning`/`.unknown`)

Engelskan delade upp den gamla `body`-nyckeln i tre: `.ended` (appen gick ner), `.keptRunning` (problemet träffade en
bakgrundstråd, appen fortsatte och användaren avslutade själv) och `.unknown` (rapport från en äldre version som inte
noterade vilket det var). `.ended` är oförändrad. De två nya får ALDRIG påstå att Cmdr kraschade, avslutades eller
stannade, och de säger `en rapport`, inte `en kraschrapport`: inget kraschade.

- **"ran into a problem" → `stötte på ett problem`** · standardkollokation i svenskan (`stöta på problem`), inget belägg
  i piles men det är för att varje pile-formulering är opersonlig: macOS AppKit skriver
  `Det inträffade ett problem med att hämta …` (`sv/macOS/AppKit/AppKitErrors.json` rad 95–98), Finder
  `ett problem inträffade med skivenheten` (`LocalizableMerged.json` `PE37`), Nautilus
  `Ett problem uppstod när detta program kördes` (`nautilus.po` rad 846). Ingen av dem kan ta `Cmdr` som subjekt, och
  alla tre systersträngarna gör det på engelska (`.ended` gör det redan på svenska: "Cmdr avslutades oväntat …"), så
  parallelliteten mellan varianterna avgör. `high`. ❌ Inte `råkade ut för`: bär en olycksnyans engelskans neutrala "ran
  into" inte har. ❌ Inte `Det inträffade ett problem i Cmdr …`: korrekt Apple-svenska, men bryter subjektet mot
  systernycklarna och blir dubbelt tungt i `.keptRunning` ("i bakgrunden i Cmdr").
- **"kept running" (appen, inte en åtgärd) → `fortsatte köra`** · macOS egen undantagsdialog har exakt verbparet:
  `sv/macOS/AppKit/NSExceptionAlert.json` `69.title` "Klicka på ”Fortsätt” om du vill fortsätta köra appen i ett
  instabilt läge" (verifierat i piles `sv/macOS/`, 2026-08-23), och Dolphin belägger intransitivt `kör` om ett program:
  "Programmet '%1' kör fortfarande i terminalpanelen" (`dolphin.po` rad 299). `high`. ❌ Inte `höll igång`: katalogens
  `hålla igång` är transitivt och är redan bokat för köns "Håll igång den här i bakgrunden", så det skulle läsas som att
  användaren gjorde något. ❌ Inte `fortsatte fungera`: `fungera` handlar om att en funktion fortsätter verka
  (termbasens "keep working" → `fortsätter fungera`), inte om att processen levde vidare, vilket är hela poängen här.
- **"in the background" → `i bakgrunden`** · den etablerade posten (Total Commander `WCMD.LNG.utf8` `1237=` "…pågående
  aktivitet(er) i bakgrunden!", `4004="I &bakgrunden"`). Ordföljden i slutfältet är plats före tid på svenska, alltså
  `i bakgrunden förra gången`, samma ordning som engelskan · `high`.
- **"a report" (utan `krasch`) → `en rapport`** · Microsoft `SWEDISH.tbx` (`report` → `rapport`, neutrum). Andra
  meningen är `.ended`:s andra mening med `krasch`-ledet borttaget, tecken för tecken, så de tre varianterna delar exakt
  samma avslutning · `high`.
- **`förra gången` behålls, trots att Apple säger `När du senast …`** · macOS renderar "The last time you opened %@, it
  unexpectedly quit …" som "När du senast öppnade %@ avslutades det oväntat …" (`sv/macOS/AppKit/AppKitErrors.json` rad
  91), och `förra gången` har noll träffar i hela piles. Men `.ended` säger redan `förra gången` och de tre nycklarna
  fyller samma plats i samma dialog: en variant som byter till en bisatskonstruktion skulle läsa som en annan mening.
  Enhetligheten vinner. Om `.ended` någon gång skrivs om är `När Cmdr senast kördes …` det belagda alternativet för alla
  tre samtidigt · `high` för valet, `tentative` för formuleringen i sig.

## Inställningstexten för rapporter gäller nu båda utfallen (`settings.updates.crashReports.description`)

Reglaget skickar en rapport även när en panic i bakgrunden INTE tog ner appen, så hjälptexten kan inte längre handla
bara om att Cmdr avslutas. Allt är hämtat från kraschdialogsavsnittet ovan, i presens:

- **`när Cmdr avslutas oväntat`** från `crashReporter.dialog.body.ended`; **`stöter på ett problem i bakgrunden`** från
  `.keptRunning` · high. Presensformen är ren morfologi, inte ett nytt termval.
- **`en rapport`** utan `krasch`-ledet, eftersom meningen täcker båda utfallen · high. ❌ ETIKETTEN
  `settings.updates.crashReports.label` står kvar som `Skicka kraschrapporter`: det är inställningens namn.
- **Andra meningen hämtad från `crashReporter.dialog.privacyNote`** (`vilken del av koden som stötte på problemet`), i
  stället för `kraschplats`, som bara stämde vid en krasch · high.

## Utmatning och frånkoppling som inte gick igenom (`errors.eject.*`)

Nio nycklar i en kort avisering uppe till höger, alltid EFTER kolon i `fileExplorer.pane.ejectFailedToast` ("Det gick
inte att mata ut {volumeName}: …") eller `.disconnectFailedToast` ("Det gick inte att koppla från: …"). Familjen är RAW,
inte ICU: vanliga apostrofer, inga dubblerade. Meningarna är korta eftersom aviseringen är liten, och de får inte
upprepa ramens "det gick inte att": ramen säger redan att det inte hände, värdet säger varför.

- **in use (om en volym eller enhet) → `används`** · macOS Finder `sv` genomgående: "Volymen kan inte matas ut eftersom
  den används" (`NE66`), "”^0” används och kan inte matas ut" (`NE31`), "En skiva på ”^0” används och kunde inte matas
  ut" (`NE79`), AppKit "Skivan kunde inte matas ut eftersom den används av ”%@”" (verifierat i pilen 2026-08-23) ·
  `high`. `unmountRefused` blir därför "Något använder fortfarande den här enheten". ❌ Inte Microsofts `upptagen`: den
  termen är bokad för badgen `(upptagen)` på menyraden (`menu.volume.ejectBusy`), där den namnger ett tillstånd, inte
  vad som pågår.
- **"Close any open files and apps" → `Stäng öppna filer och appar`** · katalogens egen vändning
  (`errors.listing.deletePending.suggestion`: "Stäng eventuella andra appar som kan ha den här filen öppen") plus macOS
  `NE52` ("Avsluta alla öppna appar och försök sedan igen") · `high`. Katalogen säger redan `stäng` om appar, så ett
  enda verb bär både filerna och apparna; `sedan` markerar ordningen precis som Apples "och försök sedan igen", vilket
  gör att meningen klarar sig utan komma mellan de två `och`.
- **removable (om en enhet) → `borttagbar`** · macOS Finder `sv` (`KIND_FORMATTER_28_0` "Removable Volume" → "Borttagbar
  volym", `KIND_FORMATTER_28_1` "Borttagbar", `GV3` "Borttagbara volymer"; verifierat i pilen 2026-08-23) · `high`.
  macOS vinner enligt Finder-regeln: Microsoft (`removable drive` → "flyttbar enhet"), Thunar och Total Commander
  ("Flyttbar enhet", "Flyttbara media") säger alla `flyttbar`, men Apple använder aldrig det ordet, och det här är
  precis Finders begrepp. Skriv `borttagbar` om enheten, inte om filer.
- **"so it stays connected" → `så den förblir ansluten`** · `förblir` är katalogens etablerade ord för ett tillstånd som
  består ("innehållet förblir låst tills du låser upp det", "mappstorlekar förblir dolda") och macOS `sv` har det med
  samma innebörd ("förblir det sparat på iCloud") · `high`.
- **unplug (fysiskt dra ur) → `koppla ur`, skilt från disconnect → `koppla från`** · katalogens egna värden
  (`errors.listing.deviceReconnecting.suggestion`: "Du behöver inte koppla ur något", `fileExplorer.mtp.deviceNotFound`:
  "Den kan ha kopplats ur", `mtp.permissionDialog.helpText`: "kopplar du ur och i enheten igen") · `high`. Om själva
  USB-kabeln säger katalogen `dra ur` (`errors.provider.macDroid.*`). De tre verben är alltså tre olika saker:
  `koppla från` (programmässigt), `koppla ur` (enheten ur porten), `dra ur` (kabeln).
- **idle (om en ansluten enhet) → `när den inte används`** · samma `används`-tråd som ovan, vilket gör hela familjen
  konsekvent · `high`. ❌ Inte Microsofts `inaktiv` (`idle` → "inaktiv"): korrekt term, men i en avisering till någon
  med en telefon i kabeln läses "när den är inaktiv" som ett tekniskt tillstånd, och `används` är redan ordet den här
  familjen använder för motsatsen.
- **"close its connection" → `stänga sin anslutning`** · Total Commander `sv` ("Vill du stänga anslutningen till '%s'?",
  verifierat i pilen 2026-08-23) · `high`. Tvåpanelslinjen är källan; macOS har ingen motsvarande sträng. "wouldn't"
  blir `ville inte`, alltså "Enheten ville inte stänga sin anslutning": aktivt, utan skuld, och utan att påstå något om
  orsaken (den skrivs till loggen).
- **eject som substantiv → `utmatningen`** · verbet `mata ut` är satt (`style.md`), och Microsoft belägger stammen i
  sammansättning (`insert/eject port` → "in-/utmatningsport") · `tentative` för den fristående substantivformen, ingen
  källa har den ensam i den här betydelsen. Behövs i `timedOut`, som följer systernyckeln `errors.mutation.timedOut`
  ordagrant ("Volymen har inte svarat än, så ändringen kan fortfarande gå igenom") och byter `ändringen` mot
  `utmatningen`. ❌ Inte "så den kan fortfarande matas ut": det läses som att DU fortfarande kan mata ut den, alltså en
  möjlighet i stället för en pågående åtgärd, och det är precis den missläsningen engelskans "on its own" undviker.
- **`unexpected` är ordagrant systernyckeln `errors.mutation.unexpected`** · samma engelska källsträng, alltså samma
  svenska: "Något gick fel och Cmdr kunde inte avgöra vad det var." · `high`. `gick fel` är den idiomatiska svenskan för
  "went wrong" och används redan genom hela katalogen; förbudet i `style.md` gäller `fel` som ETIKETT på händelsen, inte
  kollokationen.
- **"couldn't tell which device" → `kunde inte avgöra vilken enhet`** · samma `avgöra` som i `mutation.unexpected` ·
  `high`. Slutet blir `så den går inte att koppla från`: `gå att` + infinitiv i stället för passivt
  `kan inte kopplas från` (style.md:s passiv-`-s`-regel), och `Cmdr` upprepas inte i andra satsen.
- **"there's nothing to …" → `så det finns inget att …`** · katalogens `askCmdr.renameUndo.unavailable` ("Det finns
  inget att återställa … eller så är dess enhet inte ansluten") · `high`. Samma mall bär både `volumeNotFound`
  (`… inget att mata ut`) och `notAnSmbVolume` (`… inget att koppla från`).

## Papperskorgs-toasten: ångra och gå till papperskorgen (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

Toasten som visas direkt efter att filer flyttats till papperskorgen, med knapparna `Ångra` och `Gå till papperskorgen`,
plus kommandot med samma namn. Återanvänder `papperskorgen`, `enhet`, `fil/filer` och `objekt`. Nya beslut:

- **put back (flytta tillbaka ur papperskorgen) → `lägga tillbaka`** · macOS Finder `sv` Tier 1: `N153.1` (`Put Back` →
  "Lägg tillbaka"), verifierat i pilen 2026-08-27 · `high`. Det här är exakt samma åtgärd som Finders egen menypost, så
  Tier 1 vinner. ❌ Inte `återställa`: det är `restore` (se § Rollback-familjen), och katalogen använder det redan för
  namnåterställningen i `askCmdr.renameUndo.*` — två olika ytor ska inte låta likadant. ❌ Inte `flytta tillbaka`, trots
  att Finders felmening säger "kunde inte flyttas tillbaka" (`PE130_V1`) och Nautilus `sv` har "Flytta tillbaka ”%s”
  till papperskorgen": menypostens ord är det användaren ser som åtgärdens NAMN, och den vinner. Preteritum blir
  `Lade tillbaka …`, subjektslöst precis som `transfer.trash` ("Flyttade … till papperskorgen").
- **undo (knappen på toasten) → `Ångra`** · macOS `sv` `ME13`/AppKit `Undo` → "Ångra", och katalogens egen
  `askCmdr.renameUndo.undo` säger redan `Ångra` · `high`. Krockar inte med `Avbryt` (Cancel).
- **go to trash → `Gå till papperskorgen`** · katalogens `Gå till`-familj (`commands.navGoToPath.label`,
  `commands.downloadsGoToLatest.label`) plus macOS `sv` "Gå till hemmappen" (`TL_HELP_HOME`) · `high`. ⚠️ 21 tecken mot
  engelskans 11: knappen sitter i en smal toast, så överflödskontrollera den paret `Ångra` / `Gå till papperskorgen`.
  Kortformen `Papperskorgen` finns om den klipps, men då tappar knappen sitt verb.
- **"stayed in the trash" → `{skippedText} {skipped, plural, one {objekt} other {objekt}} ligger kvar i papperskorgen`**
  · `ligger kvar` är katalogens ord för något som blir stående (`fileExplorer.smb.*`: "den här delningen ligger kvar på
  systemanslutningen") · `high`. `{skipped}` är heltalspartnern till `{skippedText}`, så väljaren är äkta. Grenarna blir
  ändå identiska: `objekt` (katalogens ord för det `item` källan säger i den här halvan) är neutrum och oförändrat i
  plural, och svenska verb böjs inte för numerus. Skriv ut båda grenarna ändå, ICU kräver det.
- **"the drive you're browsing" → `enheten du bläddrar i`** · katalogens `askCmdr.empty.hint` ("det du bläddrar i") ·
  `high`. `öppna` var varmare i indexeringspasset, men här är `Öppna` redan meningens huvudverb.
- **"This drive doesn't keep a trash." → `Det finns ingen papperskorg på den här enheten.`** · samma ram som
  systersträngen `fileOperations.delete.archiveWarningStrong` ("Det finns ingen papperskorg i ett arkiv.") · `high`. Ett
  konstaterande om enheten, ingen anmärkning mot användaren.

## Komplettera en redan skickad felrapport (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

Dialogen som öppnas från toasten "Felrapporten har skickats": den visar vad som redan laddades upp och låter dig skriva
en notering som fästs på **samma** rapport (inget skickas en andra gång). Återanvänder `felrapport`, `notering`,
`Referens-ID`, `teamet` och `bifoga … e-post` från `errorReporter.json`/`common.json`-passen. Nya beslut:

- **"add to" (fästa något på en befintlig rapport) → `lägga till i`** · macOS `sv` AppKit/Finder har mönstret som
  knappetikett: "Lägg till i Dock", "Lägg till i sidofältet" (verifierat i pilen 2026-08-28) · `high`. Därför
  `Lägg till i din felrapport` (titel), `Lägg till i rapporten` (knapp), `Lägger till…` (pågår). ❌ Inte `bifoga`: det
  ordet är redan upptaget av e-postadressen (`common.attachEmail`, `settings.updates.attachEmailToReports.label`), och
  två olika "fästa vid rapporten"-verb i samma dialog läses som två olika saker. ⚠️ `Lägg till i rapporten` är 21 tecken
  mot engelskans 13 på en smal knapp; överflödskontrollera den mot `Stäng`. Kortformen `Lägg till` (macOS belägger den
  ensam) finns om den klipps, men då tappar knappen sitt objekt.
- **"What was sent" → `Det här har skickats`** · syskon till `errorReporter.dialog.detailsToggle` ("Det här är på väg
  att skickas"), enligt § Kraschdialogens tre varianter-regeln om delad ram · `high`. Perfekt, inte preteritum:
  rapporten skickades nyss och resultatet står kvar, precis som `autoSentToast.title` ("Felrapporten har skickats").
  Följs av paketstorleken inom parentes, så etiketten måste stå för sig själv.
- **"can't take a note any more" → `Det går inte längre att lägga till något i den rapporten`** · macOS `sv` säger
  genomgående `inte längre` för "no longer / not any more" ("Det här dokumentet är inte längre tillgängligt", "har du
  inte längre behörighet", verifierat i pilen 2026-08-28) · `high`. `gå att` + infinitiv i stället för passivt
  `kan inte läggas till` (style.md:s passiv-`-s`-regel). Ordföljden är `inte längre att`, inte `att … längre`.
- **Pekare till en meny → `från Hjälp-menyn`** · katalogens egen `settings.updates.errorReports.description` ("Du kan
  alltid skicka en manuell rapport från Hjälp-menyn") plus macOS `sv` "Apple-menyn" · `high`. Menyrubriken `Hjälp` är
  satt (§ Inbyggda menyer, `menu.bar.help`), och bindestrecket är det svenska sättet att sammansätta ett egennamn med
  `-menyn`. ❌ Inte `menyn Hjälp` och inte `Hjälpmenyn` utan bindestreck.
- **"To get your notes to the team, …" → `…, så når dina noteringar teamet`** · aktiv följdsats i stället för engelskans
  syftesbisats · `high`. Verbet `nå` gör noteringarna till subjekt, vilket undviker både passiv och ett upprepat
  `skicka` i samma mening som redan börjar med `Skicka en ny rapport`.
- **"it'll join what the team already has" → `så läggs det till i rapporten som teamet redan har`** · samma `teamet` som
  `errorReporter.dialog.description` ("till teamet så att vi kan åtgärda") · `high`. Engelskans "what the team already
  has" blir konkret `rapporten som teamet redan har`: det är hela poängen med dialogen (inget andra paket skickas), och
  svenskan har inget lika smidigt `vad de redan har`.
- **"Note added to your report." → `Noteringen har lagts till i din rapport.`** · syskon till
  `errorReporter.sentToast.message` ("Felrapporten har skickats. Ditt referens-ID är"), samma
  `<substantiv> har <perfekt particip>`-ram · `high`. Andra satsen är ordagrant systernyckelns.
- **"Close" → `Stäng`** · macOS AppKit `Close` → "Stäng" · `high`. Skilt från `Avfärda` (dismiss, på toaster) och
  `Avbryt` (cancel, när en åtgärd överges). Här stänger knappen bara dialogen, ingen åtgärd avbryts.
- **`errorReporter.autoSentToast.viewOrAddNotes` → `Visa rapporten eller lägg till en notering`** · båda halvorna
  bevarade (titta + lägga till), vilket engelskan uttryckligen kräver · `high`. ⚠️ 42 tecken mot engelskans 31, i en
  toast bredvid `Ändra inställningar`: överflödskontrollera. Avvisade kortformer: `Visa eller lägg till noteringar` (gör
  `noteringar` till objekt även för `visa`, men det är rapporten man visar) och `Visa eller komplettera rapporten`
  (kompakt och korrekt, men tappar ordet `notering` som binder knappen till fältet `Din notering` i dialogen).

## Markeringsdialogen: markera och avmarkera filer (`selection.*`)

- **select (filer via ett mönster) → `markera`; deselect → `avmarkera`** · macOS Finder `sv`, `MenuBar.json` `172.title`
  = ”Markera allt” och `300488.title` = ”Avmarkera allt” (Tier 1, kontrollerat mot det körande systemet, macOS 26.6.2,
  build 25G83, 2026-08-29). Microsoft Terminology säger detsamma (`deselect` → ”avmarkera”, term-id `44738`, SWE), och
  Total Commander `sv` (`WCMD.INC.utf8` rad 239–248: ”Markera alla filer”, ”Avmarkera grupp: enbart filer”) håller samma
  par i den ortodoxa tvåpanelsvärlden · `high`. Svenskan har alltså ett riktigt transitivt verb för båda hållen, till
  skillnad från tyskan och nederländskan, så hela familjen (`menu.select.*`,
  `commands.selectionSelectFiles`/`…DeselectFiles`, `selection.dialog.title.*`, `selection.action.*`) använder samma
  ordpar rakt igenom.
- **`Markera` är att markera objekt, `Välj` är att välja ett alternativ.** Redan satt i § Inbyggda menyer; det är därför
  dialogtiteln heter `Markera filer` och inte `Välj filer`.
- **recent selections → `senaste markeringar`** · speglat på syskonen i `queryUi.recent.*` (”senaste sökningar”), samma
  grammatik, bara `sökningar` → `markeringar`. `markering` är den satta termen för selection (`terms.json` `select`) ·
  `high`.
- **`selection.recent.applyAria` följer `search.recent.runAria`** · där står ”Kör senaste {mode}-sökning: {query}”, här
  ”Använd senaste {mode}-markering: {query}”. `apply` → `Använd` från macOS AppKit (`NSFontOptionsPanel` `100411.title`
  och `NSPreferences` `7TY-1Z-cs2.title` = ”Använd”) · `high`. `{query}` ligger sist efter kolon, så vilken användartext
  som helst får plats.
- **Enter-tangenten heter `Retur` på svenska** · `search.runHint` säger redan ”Tryck på Retur för att söka”, alltså
  ”Tryck på Retur för att filtrera” · `high`. Samma gäller tangentchippen (`queryUi.recent.popoverHint`,
  `queryUi.empty.tipAi`) och raderna `Vad Retur gör med …` i Inställningar > Arkiv.
- **Verktygstipset är en egen mening och behöver inte upprepa knapptexten.** `QueryDialog.svelte` bygger knappens
  tillgängliga namn av `config.primaryAction.ariaLabel ?? config.primaryAction.label`, alltså av label-nyckeln, medan
  verktygstipset sitter på ett inre `span` via `use:tooltip`. WCAG 2.5.3 är därmed uppfyllt av konstruktionen, och
  katalogen låter de två skilja sig åt på annat håll (`search.action.showAll.label` mot dess `.tooltip`). Här råkar
  svenskan flyta ihop ändå (”Markera de här filerna i den fokuserade panelen”), vilket är bra men inget krav.
  `den fokuserade panelen` kommer från `commands.navGoToPath.description` och `commands.favoritesAdd.description`;
  `de här` är katalogens närdemonstrativ (inte `dessa`, som katalogen sparar till distansbruk) · `high`.
- **`selection.notice.snapshotPane` → ”Matchningen sker mot det som visas i listan (hela sökvägen).”** · lugnande, inte
  en varning, som `@key` kräver; `hela sökvägen` är katalogens form (se `errors.listing.nameTooLongErrno.explanation`) ·
  `high`.

## Termdriftsgranskning: en sak, ett namn

Hela katalogen gicks igenom med `i18n-check-term-consistency` plus de tre manuella passen i
`docs/guides/i18n-translation.md` § "Auditing a finished locale for term drift". macOS-belägg är kontrollerade mot det
körande systemet (macOS 26.6.2, build 25G83, 2026-08-30) via `Finder.app`/`Safari.app` per-nib `.strings` och
`.loctable`, med `en_GB.lproj` som engelsk sida; pilen (`_ignored/i18n/sv/`) står för Microsofts terminologi och Tier 3.

### Samma engelska, ett svenskt ord (`menu.file.delete`, `commands.fileDelete.label`, `settings.mediaIndex.clip.*`, `commands.selectionSelectAll.label`, `commands.selectionDeselectAll.label`, `fileExplorer.errorPane.goHome`)

- **Delete (F8, till papperskorgen) → `Radera`** · macOS AppKit `sv` `"Delete": "Radera"` i fyra buntar (`Common`,
  `Document`, `FontManager`, `MenuCommands`), plus Finder `Radera direkt…` (`300770.title`) · `high`. `menu.file.delete`
  och `commands.fileDelete.label` sa `Ta bort` medan funktionstangentraden och raderingsdialogen sa `Radera` om samma
  åtgärd: menyraden och kommandopaletten döpte alltså F8 till ett annat verb än tangenten själv. `style.md` reserverar
  redan `ta bort` för att plocka bort något ur en lista. Paret ska dessutom skilja sig i STYRKA, inte i verb: `Radera` /
  `Radera permanent` gör det, `Ta bort` / `Radera permanent` gör det inte.
- **Delete (den hämtade AI-modellen) → `Radera`** · samma regel; `settings.mediaIndex.clip.*` var en `ta bort`-ö bredvid
  `ai.local.*` som redan sa `Radera modell` om exakt samma handling. `settings.mediaIndex.reclaim.*` behåller `ta bort`:
  där plockas rader ur ett index, inte filer från disken. `ai.local.deletingStatus` behåller också `tar bort filer`,
  eftersom engelskan där säger `removing`, precis som i `errors.listing.folderNotEmpty.suggestion` ("Radera innehållet i
  mappen först, och prova sedan att ta bort mappen igen").
- **Select all / Deselect all → `Markera allt` / `Avmarkera allt`** · macOS Finder `MenuBar.strings` `172.title` och
  `300488.title`, plus AppKit `MenuCommands` och `FindPanel` (`"Select All": "Markera allt"`) · `high`. Se
  `allt`-konventionen nedan.
- **Close other tabs → `Stäng övriga flikar`** · Safari 26 `sv` `MainMenu.strings` `686.title` · `high`. Samma `övriga`
  som Finders `Göm övriga`; `andra` är inte Apples ord här.
- **word wrap → `Automatiskt radbyte`** · Microsofts terminologi, term-id `134172` (SWE, substantiv) · `high`. Ersätter
  den tidigare `radbrytning`. macOS har ingen egen "word wrap": TextEdit `sv` säger `Anpassa texten till fönstret` om en
  annan funktion (Wrap to Window) och `Radbrytning` bara inne i utskriftspanelens `Radbrytning efter sidans storlek`.
  Tier 1 saknar termen, alltså avgör Tier 2.
- **Dismiss → `Avfärda`** · macOS `sv` `"Dismiss Popover": "Avfärda popover"` · `high`. Microsoft säger `stäng` (term-id
  `1633537`), men Tier 1 vinner, och `Stäng` är dessutom upptaget av Close.
- **Copied → `Kopierat`** · `high` (grammatik; ingen källa har en naken "Copied"-knapp). Supinformen fungerar oavsett
  vad som kopierades, och de två avvikarna kopierar ett `referens-id` — `ett id` är neutrum, så `Kopierad` var fel även
  på kongruensen.
- **Go to home folder → `Gå till hemmappen`** · `commands.nav*`-familjens satta `Gå till …` (`Gå till sökväg…`,
  `Gå till överordnad mapp`) · `high`. `fileExplorer.errorPane.goHome` är bokstavligen samma knapp som
  `commands.navGoHome.label`, så `Öppna hemmappen` var ren dubblering.
- **Reset all to defaults → `Återställ allt till förval`** · se `allt`-konventionen nedan · `high`. macOS
  tangentbordsinställningar säger `Återställ förval` (`KeyboardSettings.appex`, `Restore Defaults`) helt utan
  kvantifierare, så bara formen på `all` behövde avgöras.
- **dir / dirs → `mapp` / `mappar`** · `high`. `kat.` stod bredvid ett utskrivet `filer` i samma statusrad ("123 filer,
  4 kat."), och `fileExplorer.summary.dirNoun` sa redan `mapp`. Förkortningen tjänade ingen bredd som `mappar` inte
  klarar.
- **you@example.com → `du@example.com`** · macOS lokaliserar bara lokaldelen: `name@example.com` → `namn@example.com`
  (`GameCenterSettingsDeviceExpertExtension.appex` och `UsersGroupsIntentsExtension.appex`, `Localizable.loctable`) ·
  `high`. `exempel.se` är dessutom en riktig registrerbar domän, medan `example.com` är reserverad för dokumentation
  (RFC 2606).
- **"This volume doesn't support trash." → `Den här volymen saknar papperskorg.`** · obestämd form bär "det finns ingen
  sådan här", enligt § Papperskorgen i namnbytesspärrarna · `high`. Rubriken ovanför säger redan
  `Papperskorgen stöds inte`, så meningen ska inte upprepa den.
- **"Sorry, we couldn''t …" → `Tyvärr, vi kunde inte …`** · katalogens satta ram i `feedback.dialog.softFailure`,
  `viewer.image.error` och `feedback.dialog.tooLong` · `high`.
- **"confirm your email" → `bekräfta din e-postadress`** · det är adressen som bekräftas, och katalogen kallar den
  `e-postadress` (`common.attachEmailInputLabel`, `common.attachEmail`, `onboarding.stepBeta.emailNote`) · `high`.

### Samma begrepp med olika engelska (`commands.viewShowHidden.label`, `settings.fileViewer.suppressBinaryWarning.label`, `settings.fileExplorer.suppressQuickLookHint.label`)

- **Hide (imperativ) → `Göm`; hidden (adjektiv) → `dold`** · `high`. macOS Finder `sv` säger `Göm X` i elva levande
  strängar (`Göm sidofältet`, `Göm verktygsfältet`, `Göm förhandsvisning`, `Göm tillägg`, `Göm statusfältet`, …) och
  noll `Dölj`; pilen ger fjorton till, inklusive naket `"Hide": "Göm"`. Microsoft (`dölja`, term-id `61374`) och KDE
  Dolphin (`Dölj filterrad`) säger tvärtom, alltså en macOS-mot-Windows-delning där macOS vinner. Adjektivet stannar
  däremot på `dold`: Nautilus ("Om dolda filer ska visas"), Thunar ("Sortera dolda filer efter andra filer") och Total
  Commander (`5154="Visa &dolda filer …"`) är eniga, och `dolda filer` är den etablerade svenska filsystemtermen. ⚠️
  `Suppress` är ett annat engelskt verb och behåller `Dölj` (`settings.fileViewer.suppressBinaryWarning.label`,
  `settings.fileExplorer.suppressQuickLookHint.label`). Bonus: `commands.viewShowHidden.label` slipper stamupprepningen
  `dölj dolda` och heter nu `Visa eller göm dolda filer`.
- **download → `hämta` / `hämtning`** · termbasens satta term (macOS `Hämtade filer`) · `high`.
  `settings.mediaIndex.clip.*` var den enda ön av `ladda ner` / `nedladdning`, och den låg bredvid
  `ai.local.downloadModel` = `Hämta modell` för exakt samma handling.
- **"folder sizes" → `mappstorlekar`** · `high`. Fem nycklar sa redan `mappstorlekar`, tre sa `katalogstorlekar`.
  `katalog` är kvar där engelskan verkligen menar `directory` i teknisk mening (`settings.listing.directorySortMode.*`,
  `errors.git.bareRepo.suggestion`, `commands.fileCopyCurrentDirectoryPath.label`). ⚠️ Engelskan växlar själv mellan
  "folder sizes" och "directory sizes" om samma funktion; det är en `en`-sida att städa, inte en svensk.
- **share → `delad mapp`** · macOS Finder `sv` `1069.title` = `Delad mapp`, och `"Manage Shared Folder"` →
  `"Hantera delad mapp"` · `high`. Katalogen hade tre ord för en sak: `delad mapp` i prosa, `delning` i
  nätverksbläddraren, `resurs` i inställningarna. Se gränsen nedan för vad som får stå kvar.
- **Genitiv på namn som slutar på konsonant: rakt `-s`, aldrig `:s`** · `high` · `style.md` § Notes and decisions.
  `errors.provider.pCloudFuse.*` skrev `pCloud:s` i samma mening som `pClouds`. Kolon-genitiv hör till förkortningar
  (`SVT:s`), inte till ett namn som `pCloud`.

### Gränser: båda formerna är rätt, platta inte ut dem (`updates.status.checking`, `ai.local.statusRunning`, `operationLog.status.running`, `shortcuts.section.filterModified`, `menu.bar.file`, `menu.bar.view`, `menu.bar.select`, `menu.view.zoom`, `menu.window.zoom`)

Varje rad som har en engelsk källsträng i parentes motsvarar en post i `i18n-term-consistency-allowlist.json`.

- **`Checking` → `Kontrollerar` när något verifieras, `Söker` när något letas fram** (`"Checking"`) · `high`.
  `ai.cloud.checking` och `licensing.dialog.checking` prövar en nyckel man redan har; `updates.status.checking` letar
  efter en uppdatering som kanske finns. Hela uppdateringsfamiljen säger redan `Sök efter uppdateringar`
  (`menu.app.checkForUpdates`, `settings.updates.checkForUpdates`, `commands.appCheckForUpdates.label`), och
  `fileOperations.transferDialog.checkingConflicts` säger `Söker efter konflikter` av samma skäl. macOS
  `Kontrollera stavning` är verifieringssidan. Syntaktisk regel: `Checking for X` → `Söker efter X`; `Checking X` →
  `Kontrollerar X`. (`indexing.run.changeCheck` är nominal av rubrikskäl, se § Enhetsindex.)
- **`Running` → `Körs` om en process kör, `Pågår` om en åtgärd är i gång** (`"Running"`) · `high`. macOS Finder `sv` har
  minimalparet: `”^0” kan inte öppnas medan Finder körs` (`N144`) mot `en annan åtgärd pågår` (`NE82`, `RN11`) och
  `några aktiviteter fortfarande pågår` (`A17`). `ai.local.statusRunning` är servern, `operationLog.status.running` är
  filoperationen.
- **`Error` → `Problem`** (`"Error"`) · `high`. Ingen diagnostikrad med `Fel` finns kvar: en uppdateringskontroll som
  inte gick igenom skrivs som hela meningar (`updates.failure.check`). `fileExplorer.network.browser.status.error` står
  bland `Kan inte nås`, `Tidsgränsen nåddes` och `Inloggningen gick inte`, där `style.md` förbjuder etiketten `fel`.
- **`unknown` böjs efter det underförstådda huvudordet** · `high`. `fileOperations.transferProgress.sizeUnknown`
  ersätter en `storlek` (utrum) → `(okänd)`, och katalogen gör likadant där ordet står för sig: `ai.local.modelUnknown`
  = `Okänd` (en `modell`), `askCmdr.cost.unknown` = `kostnad okänd`. Allt som levereras i dag har ett utrumshuvudord och
  tar alltså `okänd`; ett neutrumhuvudord (`ett antal`, `ett fel`) skulle ta `okänt`.
- **`Modified` → `Ändrad` som attribut, `Ändrade` som filterpastill** (`"Modified"`) · `high`.
  `fileExplorer.columns.modified` och syskonen beskriver EN fils datum; `shortcuts.section.filterModified` står bredvid
  `Alla` och `Konflikter` och filtrerar en mängd kommandon, så pluralen kongruerar med mängden. Radmärket intill heter
  fortfarande `Ändrad från förval`, singular, för att det gäller en rad.
- **`Put back …` → `Lade tillbaka …` ur papperskorgen, `De gamla namnen återställdes …` för namn**
  (`fileOperations.trash.undone` respektive `askCmdr.renameUndo.undone`/`.partial`) · `high`. Redan satt i §
  Papperskorgs-toasten: `lägga tillbaka` är Finders `Put Back` (`N153.1`), och `återställa` är reserverat för
  `askCmdr.renameUndo.*`, där de gamla NAMNEN kommer tillbaka och ingenting flyttas. Engelskan delade en gång en enda
  sträng mellan de två handlingarna och har nu skilt dem åt; svenskan höll dem isär hela tiden. Exakt lydelse: § Samma
  engelska i menyn och paletten, och Systeminställningarnas paneler `en` fixes (2026-08-30) sist i filen.
- **`File` → `Arkiv` som menyradsrubrik, `Fil` överallt annars** (`"File"`) · `high`. Redan satt i § Inbyggda menyer
  (Finder `300764.title`/`83.title`, AppKit `MenuCommands`). `suggestedOps.columnFile` är en kolumnrubrik, inte en meny.
- **`View` → `Innehåll` som menyradsrubrik, `Visa` som åtgärd** (`"View"`) · `high`. Redan satt i § Inbyggda menyer
  (Finder `206.title`/`207.title` OCH Safari `200.title`). `menu.file.view`, `commands.fileView.label` och
  funktionstangentraden är F3-åtgärden.
- **`Select` → `Markera` för objekt, `Välj` för ett alternativ** (`"Select"`) · `high`. Redan satt i § Inbyggda menyer
  och § Markeringsdialogen; `ui.select.placeholder` är en rullgardins platshållare.
- **`Zoom` → `Zoom` (substantiv, textzoom-undermenyn) mot `Zooma` (verb, Fönster-menyn)** (`"Zoom"`) · `high`. Redan
  satt i § Inbyggda menyer (Finder `300667.title`).
- **`share` → `delad mapp` fritt stående, `-resurs` bara inne i en sammansättning, `delning` bara i räknaren** · `high`.
  Svenskan kan inte sammansätta en tvåordsfras, så Microsofts `-resurs` står kvar där en sammansättning krävs:
  `settings.section.smbNetworkShares` (`SMB-/nätverksresurser`), `settings.appearance.tintSmb.description`,
  `settings.network.directSmbConnection.*`, `settings.network.timeoutMode.description`,
  `settings.advanced.mountTimeout.description`, `settings.summary.smbNetworkShares` (`resurscache`),
  `settings.indexing.askForEachDrive.description`. Räknaren `fileExplorer.network.share.shareCount` behåller kortformen
  (`{countText} delning` / `delningar`), precis som `terms.json` `network-share` tillåter. Allt annat är `delad mapp` /
  `delade mappar`.

### Konvention: ett naket engelskt `All` blir `allt`, inte `alla` (`menu.select.all`, `menu.select.deselectAll`)

macOS `sv` säger `Markera allt` och `Avmarkera allt`, aldrig `alla`. Utan utsatt huvudord dinglar `alla` (alla vad?),
medan neutrumformen `allt` står för sig själv. Gäller `Select all`, `Deselect all` och `Reset all to defaults`. Med
huvudord böjs det förstås normalt (`Stäng övriga flikar`, `Ångra alla omgångar`). Också noterad i `style.md`.

### Apples egna namn: kontrollera dem mot det körande systemet, inte mot minnet (`commands.handler.zoomResetHintMenu`, `main.upgradeNudge.mac`, `errors.listing.ioSerious.suggestion`)

Den dyraste klassen av drift i det här passet var inte två svenska ord för en engelsk term, utan ett svenskt ord för
något Apple redan har döpt. Copy som PEKAR på en systemyta måste stava ytan som macOS stavar den, annars skickas
användaren att leta efter ett menyalternativ som inte finns. Tre rättade, alla verifierade live på macOS 26.6.2 (build
`25G83`, 2026-08-30):

- **`Full Disk Access` → `Full skivtillgång`** i 19 nycklar (`onboarding.stepFda.*`, `onboarding.stepAi.banner*`,
  `search.coverage.*`, `downloads.fda.message`, `common.downloadsFdaHint`, `askCmdr.wake.needsFullDiskAccess`,
  `settings.behavior.…globalGoToLatestShortcut.enabled.description`). Se `terms.json` `full-disk-access` för belägget
  och kongruensen. Det som gjorde felet osynligt: `{full_disk_access}` i `errors.*` hämtas från OS:et i körningen, så
  appen visade Apples namn på ett ställe och en egen omskrivning på nitton andra.
- **`View > Zoom > 100%` → `Innehåll > Zoom > 100 %`** (`commands.handler.zoomResetHintMenu`). Tipset pekade på
  `Visa > Zooma`, alltså två menyer som inte finns: menyradsrubriken heter `Innehåll` och textzoom-undermenyn `Zoom`
  (`Zooma` är verbet i Fönster-menyn). Båda gränserna stod redan i § Inbyggda menyer; det var bara den här strängen som
  inte följde dem.
- **`Cmdr > Onboarding…` → `Cmdr > Introduktion…`** (`main.upgradeNudge.mac`). `menu.app.onboarding` heter
  `Introduktion…`, så aviseringen namngav menyposten på engelska.
- **`Disk Utility > First Aid` → `Skivverktyg > Skivkontroll`** (`errors.listing.ioSerious.suggestion`).

Kontrollerade och redan rätt: `Systeminställningar > AI`, `Indexering > Enhetsindexering`,
`Hjälp > Skicka återkoppling…`, `Inställningar > Tangentbordsgenvägar`, `Inställningar > Uppdateringar och integritet`,
`Visa info` (Get Info), `Nyckelhanterare`, `Integritet och säkerhet`.

**Regel:** varje gång en sträng skriver ut ett menyalternativ, en inställningspanel eller ett Apple-funktionsnamn, slå
upp det i det körande systemet (`how-to-mine.md` § "Menu-bar labels" och `.loctable`-receptet) och datera fyndet. Ett
namn som "låter rätt" är den enda sortens fel användaren kan följa rakt in i en återvändsgränd.

## Samma engelska i menyn och paletten, och Systeminställningarnas paneler (`menu.app.hideOthers`, `commands.appHideOthers.label`, `menu.app.showAll`, `askCmdr.renameUndo.undone`/`.partial`)

Fallout from four `en` self-inconsistency fixes. Evidence is macOS 26.6.2 (build 25G83), read live off the installed
bundles with the `.loctable` / `MenuBar.strings` recipes in `docs/i18n/reference-pile/how-to-mine.md`, 2026-08-30.

- **`Hide others` (app menu) → `Göm övriga`** · Tier 1, three independent bundles agree: Finder `MenuBar.strings`
  `300729.title`, TextEdit `Edit.loctable` `515.title`, Preview `MainMenu.loctable` `145.title`. `menu.app.hideOthers`
  already said this; `commands.appHideOthers.label` said `Göm andra` and now matches, since the two name the same
  command (menu bar vs palette and shortcuts list). ❌ Not `Göm andra`: the OS word is `övriga`. · `confirmed`
- **`Show all` (app menu) → `Visa alla`** · same three bundles (`300730.title` / `517.title` / `150.title`), plus AppKit
  `Common.loctable`. Already shipped, unchanged. Swedish sentence case is native, so Cmdr's sentence-case menu bar needs
  no deviation from Apple's wording here. · `confirmed`
- **`Login Items & Extensions` (System Settings pane) → `Startobjekt och tillägg`** · macOS
  `LoginItems.appex/Contents/Resources/Localizable.loctable`, English-keyed `Login Items & Extensions`. ❌ Not
  `Inloggningsobjekt och tillägg`, which the catalog shipped: that string isn't in the OS, so a user following the path
  would hunt for a pane that doesn't exist. `General` → `Allmänt` (SystemSettings `GENERAL`) and `Apple Account` →
  `Apple-konto` (`ClassKitSettings.loctable` `APPLE_ID`) were already right. · `confirmed`
- **System Settings panes via tokens in the git and provider errors** · the eight `errors.git.*` / `errors.provider.*`
  suggestions now carry `{system_settings}` / `{privacy_and_security}` / `{files_and_folders}`, the same
  runtime-resolved placeholders the `errors.listing.*` family already used. Never hand-translate them, and never hang a
  suffix or preposition off one: write `i {system_settings}`, never `{system_settings}en`. The literals
  `Systeminställningar`, `Integritet och säkerhet`, and `Filer och mappar` are gone from those strings. · `high`
- **"Put the old names back on N files" → `De gamla namnen återställdes på {countText} filer.`**
  (`askCmdr.renameUndo.undone` / `.partial`) · the English now names the OBJECT (the old name), so the old Swedish
  ("{countText} filer återställdes.") no longer said what came back. Keeps the split already settled in §
  Papperskorgs-toasten: `återställa` for restoring a NAME, `lägga tillbaka` for Finder's `Put Back` out of the trash
  (`fileOperations.trash.undone` is untouched and still says `Lade tillbaka …`). Both plural branches are spelled out
  because the noun and the participle agree: `Det gamla namnet återställdes … fil` /
  `De gamla namnen återställdes … filer`. Passive `-s` matches the sibling `undoing` ("De gamla namnen återställs…"). ·
  `high`
- **`settings.indexing.enabled.description`** · English switched "directory sizes" → "folder sizes"; Swedish already
  said `mappstorlekar`, so this was a restamp only. · `high`
- **Email placeholder** · `du@example.com` on all three keys (`settings.updates.emailPlaceholder`,
  `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`), already consistent. `du` is the catalog's
  pronoun; `example.com` is the RFC 2606 reserved domain and stays. · `high`

## En halvt ångrad åtgärd: att ångra klart (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- **`Finish rolling back` → `Ångra klart`** · partikelverbet bygger på den satta rollback-termen `ångra` (§
  Rollback-familjen) och på macOS Finder `sv`, som skriver precis den konstruktionen om en avbruten filoperation: ”Du
  kan kopiera klart nu eller behålla en återupptagbar kopia och slutföra senare.” Katalogen har redan samma mönster i
  `errors.listing.connectionDropped.explanation` (”innan Cmdr hann läsa klart”) och
  `fileOperations.transferDialog.scanStopped` (”kunde inte mäta klart källan”) · `high`. Formen säger ”göra färdigt det
  som stannade”, aldrig ”börja om”.
- **❌ Inte `Slutför ångringen`.** Finder `sv` har visserligen knappformen `Slutför kopiering` (och
  `Slutför komprimering`), så substantivmönstret är belagt, men två saker fäller det: § Rollback-familjen har redan dömt
  ut substantivet `ångring` som ”korrekt men styltigt i ett gränssnitt” (det var därför `transferProgress.smbNativeNote`
  skrevs om till verbform), och `@key` säger uttryckligen att knappen ska vara kort eftersom den sitter inne i en
  listrad — `Ångra klart` är 11 tecken mot 17. Det är ett belagt alternativ om någon vill byta, men då byter hela paret
  samtidigt.
- **De två `finishRollBack`-nycklarna måste vara identiska tecken för tecken.** `operationLog.dialog.finishRollBack` och
  `fileOperations.rollbackConfirm.finishRollBack` har samma engelska sträng och samma handling (raden öppnar just den
  dialogen), så `i18n-terms` varnar om de får två olika svenska namn. Skriv om båda eller ingen.
- **`Finish rolling this back?` → `Ångra klart den här åtgärden?`** · samma ram som syskonet
  `fileOperations.rollbackConfirm.title` (”Ångra den här åtgärden?”), bara med partikeln inskjuten: bar infinitiv plus
  frågetecken, som `main.quit.title` · `high`. Objektet står efter partikeln (”ångra klart åtgärden”, precis som ”läsa
  klart boken”).
- **Knappen `Ångra klart` och pastillen `Ångrad` krockar inte.** De står aldrig på samma rad: raden bär antingen
  `Delvis ångrad` plus knappen, eller `Ångrad` utan knapp. Efter tryck läses växlingen som framsteg.
- **Notisen ekar `fileOperations.rollbackConfirm.bodyUndoByDeleting`** · ”Cmdr ångrade det som gick och lät resten ligga
  kvar. Att ångra klart går igenom åtgärden en gång till och hoppar över allt som fortfarande är osäkert.”
  `hoppar över allt som … är osäkert` är ordagrant från `bodyUndoByDeleting`, `ligga kvar` speglar dess ”så en del kan
  bli kvar”, och `lät resten ligga kvar` håller tonen från `rollbackConfirm.leaveAsIs` (”Låt det vara”) · `high`.
  Meningen lovar medvetet ingen fullständig ångring.
- **❌ Inte ”lät resten vara som den var” i notisen.** Engelskans ”left the rest as it was” kan läsas som ”som det var
  före åtgärden”, alltså tvärtemot vad som hänt. `ligga kvar` säger otvetydigt att resten ligger där åtgärden lämnade
  den.
- **”takes another pass” → `går igenom åtgärden en gång till`** · `@key` glossar uttrycket som ”goes over the operation
  once more”, och den formen är vanlig svenska · `high`. `tar ett varv till` fungerar också men säger inte vad som gås
  igenom.
- **Inget komma före `och` i första meningen** · satserna delar subjekt (Cmdr), och style.md:s regel om två korta
  huvudsatser gäller ändå · `high`.
- **`in {folder}` → `i {folder}`** · katalogens egen preposition för att arbeta inne i en mapp
  (`fileOperations.operationConflict.context` = ”Arbetar i {destination}”) · `high`. Raden läses ”Raderar det som
  skapades i Backup”, alltså som en bestämning av var sakerna skapades, vilket är precis den läsning nyckeln finns för.
  Inga citattecken behövs: svenskan sätter ingen kasusändelse på namnet, så vilket mappnamn som helst passar.

## Toasten efter en ångrad kopia eller flytt (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

Toasten som rapporterar vad ångringen hann med: en rubrik (hel, delvis, eller stoppad ångring), raden som sätter
förväntan, och en punktlista med ett skäl per rad. Raderna är nästan en kopia av `askCmdr.renameUndo.skipReason.*`, och
två av dem har teckenidentisk engelska, så `i18n-terms` kräver identisk svenska. Nya beslut:

- **”Left {name} alone” → `{name} lämnades som den är`** · redan satt i katalogen
  (`askCmdr.renameUndo.skipReason.drift`/`.unverifiable`/`.folderNotEmpty`), och `folderNotEmpty.named`/`.counted` har
  dessutom exakt samma engelska sträng som sina renameUndo-syskon, så de två värdena är kopierade tecken för tecken.
  `unverifiable.named` är nästan identisk men inte riktigt (apostrofen är böjd i `renameUndo`, dubblerad i
  `cancelRollback`), så `i18n-terms` binder den inte — svenskan är ändå densamma, för det är samma mening. macOS `sv`
  belägger verbet i samma betydelse: ”Om du vill lämna filen orörd och jobba med en kopia klickar du på Duplicera” ·
  `high`. ❌ Inte `Lät {name} vara` (som annars hade knutit an till knappen `Låt det vara`): det hade gett en engelsk
  mening två svenska namn.
- **”Left {name} where it is” (`spotTaken`) → `{name} lämnades där den ligger`** · engelskan byter medvetet ram just
  här, eftersom ”lämna i fred” vid en flytt betyder att objektet blir kvar på det NYA stället; svenskan byter med ·
  `high`.
- **”something else now sits where it came from” → `något annat finns nu där den kom ifrån`** · `kom ifrån` är
  katalogens egen formulering för ursprungsplatsen (`rollbackConfirm.bodyUndoByMovingBack`: ”dit de kom ifrån”) ·
  `high`. ❌ Inte `platsen den kom ifrån är upptagen`, trots att macOS `sv` belägger `upptaget` om ett taget namn
  (”minst ett av dem med namnet ”^0” är upptaget”): den formen låter likadant som grannskälet `nameTaken` i
  `askCmdr.renameUndo` (”det gamla namnet är taget igen”), och två olika skäl ska gå att skilja åt. `finns nu` i stället
  för `ligger nu` bara för att inte säga `ligger` två gånger i samma rad.
- **”it changed after Cmdr put it there” → `den ändrades efter att Cmdr lade den där`** · systerraden
  `askCmdr.renameUndo.skipReason.drift.named` säger `den ändrades efter namnbytet`, så bara bestämningen byts ut.
  `lade den där` håller sig i `lägga`-familjen (`Lade tillbaka …`) och täcker både kopian som skrev filen och flytten
  som bar den dit · `high`.
- **”Removed …” (det ångringen tar bort) → `Raderade …`** · `radera` är katalogens ord för att ta bort filer från disk
  (`terms.json` `delete`), och alla tre `rollbackConfirm.bodyUndo*` säger redan `raderar` om exakt den här handlingen ·
  `high`. ❌ Inte `Tog bort`: `ta bort` är reserverat för att plocka bort något ur en lista. Samma svenska som
  `operationLog.summary.delete` (”Raderade {countText} objekt”), fast engelskan där säger `Deleted`: svenskan skiljer
  inte på `remove` och `delete` när det gäller filer på disk.
- **”Put … back” → `Lade tillbaka …`** · `fileOperations.trash.undone` säger redan `Lade tillbaka` om samma handling,
  och Finders menypost `Lägg tillbaka` är källan (§ Papperskorgs-toasten) · `high`.
- **Bestämd totalitet (”the {countText} items”) → `allt …: {countText} objekt`** · svenskan kan inte sätta bestämd
  artikel framför en `*Text`-placeholder, så hela och delvisa rubriker skiljs åt med `allt` plus kolon:
  `Raderade allt Cmdr hade skrivit: {countText} objekt` mot `Raderade {countText} objekt`, och
  `Lade tillbaka allt: {countText} objekt` mot `Lade tillbaka {countText} objekt`. Konventionen och varför artikeln inte
  går: style.md § Plurals · `high`.
- **”Stopped after removing …” → `Stoppade efter att ha raderat …`** · `Stoppa` är det satta verbet för att stoppa
  själva ångringen (§ Frågan som stoppar en kö-rad; macOS Finder ”stoppa processen och behålla en delvis kopia”) ·
  `high`. Subjektslöst preteritum som resten av toastfamiljen.
- **”The rest are still there.” → `Resten ligger kvar.`, ”The rest stayed where the move put them.” →
  `Resten ligger kvar där flytten lade dem.`** · `ligga kvar` är katalogens ord för något som blir stående
  (`fileExplorer.pane.directConnection*`, `trash.undonePartial`), och `flytten` är katalogens substantiv för själva
  flyttåtgärden (”Enheten kopplades från under flytten”) · `high`. Den korta varianten räcker för kopian: `ligga kvar`
  säger redan ”där de är”, och det är bara flytten som behöver peka ut vilken plats.
- **`leftBehind`: ”so these stayed where they are:” → `så det här blev kvar:`** · första halvan är ordagrant
  `rollbackConfirm.body`/`bodyUndo*` (`Cmdr hoppar över allt som är osäkert`) och andra halvan är deras egen svans
  (`så en del kan bli kvar`) i preteritum, så toasten läses som samma utfästelse som dialogen användaren nyss läste ·
  `high`. ❌ Inte `så det här ligger kvar där det är:` — pleonasm, och raden står direkt under `Resten ligger kvar`.
- **”Couldn't undo {name}.” → `Det gick inte att ångra {name}.`** · katalogens standardram för något som inte gick
  igenom (`errors.eject.*`, `fileExplorer.pane.directConnection*`), och den lägger inte skulden på Cmdr när det är
  enheten som sa nej · `high`. `Cmdr kunde inte ångra …` (som `askCmdr.renameUndo.refusedBatches`) är också gångbart,
  men där har engelskan `Cmdr` utsatt och här inte.
- **”Its drive may be disconnected or read-only.” → `Enheten den ligger på kan vara frånkopplad eller skrivskyddad.`** ·
  `enhet`, `koppla från` → `frånkopplad` och `skrivskyddad` är alla satta sedan tidigare · `high`. `dess enhet` finns i
  katalogen (`trash.undoUnavailable`), men blir styltigt först i en mening.
- **`rollbackConfirm.body` fick engelskans nya tredje löfte** · andra meningen är nu teckenidentisk med
  `bodyUndoByDeleting` (”Cmdr hoppar över allt som är osäkert, så en del kan bli kvar.”), enligt style.md § Notes and
  decisions om att syskonvarianter delar ram. Kommat före `och` står kvar: båda satserna är långa nog att läsaren
  behöver pausen.
- `item` → `objekt` och `folder` → `mapp`/`mappar` oförändrat. `objekt` och verben böjs inte för numerus, så båda
  ICU-grenarna blir identiska; de skrivs ut ändå.

### `cancelRollback.stagedLeftover.*` (Cmdrs egna rester på målplatsen)

Nya 2026-09-02. Två rader om en arbetsfil som Cmdr själv skapat och inte lyckats ta bort från målplatsen. De hör INTE
till `reason.*`-listan: där skyddar Cmdr användarens filer, här handlar det om Cmdrs egen rest.

- **`unfinished copy` → `ofullständig kopia`** · `ofullständig` är Apples ord för "incomplete" (macOS `LA33`: "skadad
  eller ofullständig"), `kopia` substantivet i `NE111` ("behålla en återupptagbar kopia") · `high`
- **`at the destination` → `på målplatsen`** · katalogens ord (`conflictsUnknown`, `stallWaitingDestination`) · `high`
- **`transfer` (substantiv) → `överföring`** · katalogen säger redan så
  (`errors.listing.deviceReconnecting.explanation`: "efter en avbruten överföring") · `high`
- **`Cmdr clears it` → `Cmdr rensar bort den`** · `rensa bort` i stället för `radera`, eftersom det är Cmdrs egen
  arbetsfil och inte användarens · `high`
- ⚠️ **`vid en senare överföring`, ❌ aldrig "nästa gång".** Cmdrs rensning hoppar över allt som är yngre än en timme,
  så ett omedelbart nytt försök rensar ingenting. Ett löfte som inte håller är precis det fel den här raden finns för
  att ta bort.

## Blockskärmen när WebKit är för gammalt (`main.oldWebkit.*`)

Tre strängar som Cmdr visar i stället för sitt gränssnitt när Macens Safari är för gammalt. De bor i HTML-skalet, inte i
appen, så det här är det enda personen kommer att se av Cmdr.

- **`Software Update` → `Programuppdatering`** · macOS namn på panelen i Systeminställningar; Tier 1-spåret i Finder
  bekräftar ordet (`Apple Device Software Update File` → `Programuppdateringsfil för Apple-enhet`) · `high`. Inte
  `Mjukvaruuppdatering`, som är den engelskpåverkade formen.
- **`Quit` → `Avsluta`** · macOS AppKit-nyckeln `Quit` → `Avsluta` · `high`. Saknades i termbasen, står här nu.
- **Genitiv på varumärket går bra på svenska: `Cmdrs gränssnitt`.** (Tyskan förbjuder sitt `Cmdrs`; det är en tysk
  regel, inte en allmän.)
- **`Safari`, `Mac` och `15.4` står kvar.** `Safari` ligger nu i `BRAND_WORDS`.

## Aviseringen om gammal macOS (`main.oldMacos.*`)

En engångsdialog på en Mac under macOS 12: Cmdr startar, men ligger utanför det testade spannet. Tonen är ärlig och
avspänd, varken ursäkt eller varning, för appen fungerar ju.

- **`supported` → `har stöd för` / `stöds`** · macOS Finder (`… eftersom den inte stöds.`) · `high`.
- **`X and up` → `X och senare`** · macOS SystemSettings (`… kräver OS X %@ eller senare.`) · `high`.
- **`best effort` → `så gott det går`** · pilen har ingen term för det (bara QoS-definitioner för nätverk) · `high` för
  omskrivningen. Medvetet ingen översättningslån.
- **`look off` → `se konstiga ut`** · vardagligt, och det undviker `fel`/`misslyckades` som rösten förbjuder.
- **Inget komma före `och` mellan de två korta huvudsatserna** i mening två, enligt § Notes and decisions i `style.md`.
- **Sista meningen är David i jag-form**, med `du`, som `onboarding.stepBeta.greeting`.

## Ask Cmdr tittar in i filer: samtyckestexten och verktygsraden (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

Ask Cmdr kan nu läsa en begränsad del av en fil på begäran (`inspect_file`): några rader ur en textfil, några PDF-sidor
med titel och författare, fillistan i ett arkiv, eller en bilds EXIF-uppgifter med plats. `contentsRule` ersätter den
gamla `consent.noContents`; dess två sista meningar (bildsökningen; förslag väntar på godkännande) är återanvända
ordagrant från den gamla översättningen, med `Fotosökningen`/`fotona` justerade till `Bildsökningen`/`bilderna` enligt
kvalitetspassets beslut (`photo` → `bild`, uniformt). Återanvänder `arkiv`, `bild`/`bilder`, `leverantör`, `tagg(ar)`,
`rad`/`rader`, `namnbyte`, `upprensningar`, `bildsökningen`. Nya/satta termer:

- **look inside (a file) → `titta in i`** · verktygsraden `Tittar in i filer` / `Tittade in i filer` följer syskonen
  `Tittar på ett förslag`/`Tittade på ett förslag` och `Läser vad som finns i dina bilder` (samma verbform, samma
  längdklass); den borttagna nyhetstexten (askCmdr.consent.whatsNew.body) sa `titta in i en fil du frågar om`. Inget
  belägg i högen för just den här AI-frasen; vald för att `titta in i` är den idiomatiska svenskan för ”look into” och
  tydlig med ett filobjekt efter sig (utan objekt kan `titta in` betyda ”hälsa på”, men den läsningen finns inte här).
  `inuti` står kvar som preposition i löpande text (`consent.memory`: ”känt igen inuti dina foton”) · `tentative`
  (stilval utan direkt belägg; syskonstyrt).
- **thumbnail → `miniatyr`** (neutrum; plural `miniatyrer`) · MS-terminologi (`thumbnail` → `miniatyr`, neutrum), macOS
  AppKit (`Miniatyrstorlek:`, ”liten/medelstor/stor miniatyrstorlek”), Total Commander (`&Miniatyrer`, ”Läs in markerade
  miniatyrer på nytt”). Nautilus/Thunar säger `miniatyrbilder`; det kortare `miniatyr` är förstapartsordet och det gamla
  `noContents` använde redan `miniatyrer` · `high`.
- **camera details (a photo's EXIF fields: model, lens, exposure) → `kamerauppgifter`** · `kamera` är belagt överallt
  (MS `camera` → `kamera`, macOS `NSStillCameraTemplate` → `kamera`, Thunar `Kamera`, Nautilus `Kameramärke`/
  `Kameramodell`); efterledet `-uppgifter` är katalogens eget ord för filmetadata
  (`askCmdr.renameReview.evidence.metadata` = ”Filuppgifter, inte innehållet”, `filuppgifterna`), så sammansättningen
  läses som ”bildens kamera-metadata”. ❌ Inte `kameradetaljer`: `detaljer` är reserverat för det expanderbara avsnittet
  (`tekniska detaljer`) · `high` (belagd förled + katalogets efterled).
- **location (where a photo was taken) → `var den togs`**, inte `plats` · `plats` är det satta ordet för `location` i
  filsystemsmening (macOS Finder `Plats`, MS `plats`, Nautilus/Thunar/Dolphin `Plats`), och direkt efter ”en bilds”
  skulle `plats` läsas som var FILEN ligger, vilket är precis det samtyckestexten inte handlar om. Bisatsen säger vad
  engelskans ”location”/”where it was taken” faktiskt betyder: `en bilds kamerauppgifter och var den togs`
  (`item.contents` och den borttagna nyhetstexten), `inklusive var den togs` (`contentsRule`) · `high` (belagd term
  medvetet undviken; bisatsen är entydig).
- **title and author (a PDF's document metadata) → `titel och författare`** · `titel`: macOS AppKit `Title` → `Titel`,
  katalogen (`Chattens titel`); `författare`: MS-terminologi (`author` → `författare`). Dolphin säger `Upphovsman`, som
  är könsmarkerat och utgår; MS:s `title` → `äganderätt` är den juridiska betydelsen och fel här.
  `tillsammans med dess titel och författare` · `high`.
- **page(s) (of a PDF) → `sida`/`sidor`** · MS-terminologi (`page` → `sida`); `några sidor ur en PDF`, `PDF-sidor`
  (bindestreck efter initialförkortning som i `zip-arkiv`, `API-nyckel`). macOS Finders `Pages` är appnamnet, inte ordet
  · `high`.
- **the list of files inside an archive → `vilka filer som finns i ett arkiv`**; what's inside an archive →
  `vad som finns i ett arkiv` · `listan över filerna i ett arkiv` är en kalk; svenskan säger naturligt ”vilka filer som
  finns i”. Samma ram (`… som finns i ett arkiv`) i alla tre nycklarna så att listan och stycket läses som en röst ·
  `high` (idiomatisk omskrivning; `arkiv` satt sedan arkivpasset).
- **some text / some lines of text / some of its text → `lite text` / `några rader text` / `en del av texten`** · tre
  engelska formuleringar, tre svenska efter sammanhang: listpunkten (`lite text`), stycket (`några rader text`, `rad` =
  textrad enligt visarpasset), nyhetsrutan (`en del av texten`) · `high`.
- **whole files → `hela filer`** · ”Cmdr skickar aldrig hela filer, bilder eller miniatyrer”; det gamla
  `själva filerna: inget filinnehåll` gick inte att behålla eftersom texten nu just LOVAR att en del innehåll skickas ·
  `high`.
- **`it` (= Cmdr) i meningen ”it can read a limited part of it” → `Cmdr`** · katalogkonventionen (Ask Cmdr-passet:
  upprepa Cmdr i stället för ett pronomen när meningen beskriver Cmdrs eget beteende); här hade `den … av den` dessutom
  syftat på två olika saker. `så att den kan hitta dem` står kvar: `den` = leverantören i samma mening · `high`.
- **Uppräkning vars sista led själva innehåller `och` → `samt` före sista ledet** · `item.contents`: ”lite text,
  PDF-sidor, vad som finns i ett arkiv samt en bilds kamerauppgifter och var den togs”. Utan `samt` hade det blivit ”… i
  ett arkiv och en bilds … och var den togs”, två `och` i rad med olika räckvidd. Regel: style.md § Notes and decisions
  · `high`.
- Kommat före `eller` i `contentsRule` (”…, vilka filer som finns i ett arkiv, eller en bilds kamerauppgifter, …”) står
  kvar: leden är långa och läsaren behöver pausen (style.md § komma före `och`/`eller`). I den borttagna nyhetstexten
  var leden kortare och kommat borta.
- **Uppföljning, samma pass: `askCmdr.empty.hint` och `settings.askCmdr.intro`** · båda bar det gamla löftet ”aldrig
  filinnehåll”/”är skrivskyddad … ändrar aldrig något”, som inte längre stämmer. Första meningen i vardera behålls
  ordagrant; andra meningen säger nu de tre faktan med de satta orden: `läser namn, sökvägar och storlekar`,
  `tittar in i en fil bara när du frågar om den` (samma `titta in i` som verktygsraden och den borttagna nyhetstexten),
  och `ändrar aldrig en fil utan ditt godkännande` (`godkänna` som i `contentsRule`: ”förrän du godkänner det”). ❌ Inte
  `skrivskyddad`/`ändrar aldrig något`: den skriver egna anteckningar och föreslår namnbyten. Kommat före det sista
  `och` står kvar i båda: leden är långa, och i `empty.hint` skiljer det sats-`och` från uppräknings-`och` · `high`.

## Rollback-knappens två knappbeskrivningar (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`/`.rollbackAlreadyLandedTooltip`)

Ny yta: knappbeskrivningen säger nu vad just DEN här ångringen gör med filerna, och knappen stängs av så snart en flytt
mellan två filsystem har nått sitt sista steg (originalen tas bort, allt ligger redan på målet).

- **`rollbackTooltipStopAndMoveBack` → `Stoppa och lägg tillbaka alla filer som flyttats hittills`** · syskonet
  `rollbackTooltip` ger ramen (`Stoppa och …`), och `lägga tillbaka` är katalogens satta verb för att flytta något till
  sin gamla plats (`cancelRollback.doneMovingBack`: ”Lade tillbaka allt”) · `high`. ❌ Inte `radera`: en flytts ångring
  tar inte bort något.
- **`rollbackAlreadyLandedTooltip`** · första satsen tar samma bild som `cancelRollback.moveAlreadyLanded` (”finns redan
  på målet”), `ångra` är det satta verbet för rollback (`rollbackUnavailableTooltip`: ”går inte att ångra”), och
  `Avbryt` är knappens egen etikett (`fileOperations.button.cancel`), så den står oböjd · `high`.

## ”Öppna terminal här” och dess appväljare (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

Ny yta: ett kort i `Beteende > Navigering och filåtgärder` som väljer vilken terminalapp kommandot startar. Listan byggs
av macOS, så bara etiketterna översätts här.

- **terminal (appklassen) → `terminal`; Terminal (Apples app) → `Terminal`** · Apples svenska Finder behåller namnet
  engelskt (`Öppna i Terminal`, nyckel `N67` i `macOS/Finder/LocalizableMerged.json`), och det generiska svenska ordet
  är samma lånord · `high`. Därför bär kortrubriken `settings.navigationAndFileOps.card.terminal` en
  `sameAsSourceJustification`: den är avsiktligt identisk med engelskan.
- **Open terminal here (kommandonamnet) → `Öppna terminal här`** · byggt på Apples `Öppna i Terminal`, med `här` för
  platsen · `high`. När kommandot självt översätts (meny, kommandopalett) måste exakt den formen användas.
- **Choose an app… → `Välj app…`** · ordagrant Apples eget `Choose Application…` (nyckel `N137`) i svensk Finder, som
  redan säger `app` · `confirmed`.
- **terminal app → `terminalapp`** · sammansatt, som katalogens övriga `app`-sammansättningar · `high`. Inga apostrofer
  i värdena, så ICU-dubbleringen `''` blir aldrig aktuell.

## `Sort by relevance`: the search-results column tooltip (`fileExplorer.columns.sortByRelevance`)

New surface: the hover tooltip on the active column header of a search-results pane. The next click puts the rows back
into the search engine's own best-match-first order.

- **relevance (how well a result matches the search) → `relevans`** · all four macOS sources agree: WorkflowKit
  (`Relevance (WFSearchSortOrder)` → `Relevans`), AppStoreKit (`SEARCH_FACET_RELEVANCE` → `Relevans`), Automator
  (`%1$[Relevans]@ …`), and Musik · `high`. Indefinite form, matching the sibling sort labels (`efter namn`,
  `efter storlek`). (verified on macOS 26.6.2 build 25G83, `plutil` dump of the shipped localizations, 2026-09-06)
- **Sentence frame → `Sortera efter relevans`** · exactly the pattern of its sibling keys in `commands.json`
  (`Sortera efter namn`, `Sortera efter storlek`) · `high`. No `sameAsSourceJustification`, and the value carries no
  apostrophe.

## `Documents and packages`: the new OOXML row (`settings.archives.ooxml.*`)

New surface: a row in the same card as `Zip-arkiv`, above the `Appaket` card. It deliberately covers BOTH Office
documents (.docx, .xlsx, .pptx) and app packages (.jar, .apk), which is why even the English avoids naming Office.

- **documents (the file kind) → `Dokument`** · macOS Finder (`TL6`/`GROUP_DOCUMENTS` → `Dokument`; kinds `RTF-dokument`,
  `Rent textdokument`) · `high`. Indefinite plural, which in Swedish is the bare form.
- **packages (generic, not only apps) → `paket`** · macOS Finder (`Visa paketets innehåll`) and the termbase's
  `bundle → paket` · `high`. Bare `paket` keeps the row broader than the `Appaket` card below it, mirroring English's
  own `packages` vs `app bundles` split.
- **Sentence frame → `Vad Retur gör med en …, … eller ….`** · the frame of the sibling keys
  `settings.archives.zip.description` and `settings.archives.bundle.description`, with no comma before `eller` (style.md
  § Notes and decisions) · `high`. The Enter key is `Retur`, as everywhere else in the catalog.

## Serverhubben: anslutningsläget, avvisningarna och glöm-dialogerna (`servers.paneState.*`, `servers.refusal.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`)

Ny yta: en panelvy som visar hur en serveranslutning går (`servers.paneState.*`), tio texter som säger varför servern sa
nej (`servers.refusal.*`), prickens knappbeskrivningar i volymväljaren, och de två bekräftelsedialogerna för att glömma
en server respektive dess sparade lösenord.

Belägget kommer från de LEVANDE macOS-paketen, inte från referenshögen: `_ignored/i18n/` finns inte på den här maskinen
(den är gitignorerad och ligger bara i en klon), och `docs/i18n/reference-pile/how-to-mine.md` § ”No pile on this
machine?” är den dokumenterade reservvägen. Allt nedan är läst på macOS 26.6.2, build 25G83, 2026-09-06.

- **Connecting to X… → `Ansluter till {name}…`** · svensk Finder `LocalizableMerged.strings` `MN1` = ”Ansluter till ^0…”
  (och `PW28` = ”Ansluter till server”). Katalogen säger redan samma sak i `fileExplorer.network.share.connecting` ·
  `high`.
- **server address → `serveradress`** · Finder `ConnectToWindow.strings` `YEA-3L-WnW.placeholderString` =
  ”Serveradress”; katalogen har det redan i `errors.listing.connectionRefused.suggestion` (”Kontrollera att
  serveradressen och porten stämmer”) · `high`. Arkets fält heter kortare `Adress` (`servers.sheet.address`), eftersom
  sammanhanget redan är givet där.
- **disconnect → `koppla från`** · Finder `LocalizableMerged.strings` `MR10.1` = ”Koppla från”, och katalogens
  `fileExplorer.unreachable.disconnect`/`servers.paneState.disconnect` säger samma · `high`. Aria-etiketten
  `fileExplorer.navigation.disconnectPlaceAriaLabel` blir därför `Koppla från {name}`, byggd precis som systerraden
  `ejectVolumeAriaLabel` (”Mata ut {name}”). Delsträngen som uppfyller WCAG 2.5.3 är `Koppla från`, ordagrant och i
  ordning.
- **Keychain Access (appnamnet) → `Nyckelhanterare`** · `Keychain Access.app/Contents/Resources/InfoPlist.loctable`,
  `sv` → `CFBundleDisplayName` = ”Nyckelhanterare”; samma ord i `SecurityInterface.framework` sv (”öppna certifikatet i
  Nyckelhanterare”). Katalogens `ai.secretError.keychainBody` använder det redan · `confirmed`. `nyckelring` är
  BEHÅLLAREN inuti appen (`InfoPlist.loctable` `keychain` = ”nyckelring”), inte appen.
- **trust (verb) → `lita på`; trusted → `betrodd`/`betrott`** ·
  `SecurityInterface.framework/Resources/Localizable.loctable` sv: ”Vill du att datorn ska lita på certifikat som
  signerats av ”%@”…”, ”Det här certifikatet märks som betrott…”, knappen `Trust` = ”Lita på” · `high`. Därav
  `servers.refusal.certificateUntrusted` = ”macOS litar inte på …” och `hostKeyUntrusted` = ”Cmdr litar inte på …”.
- **certificate → `certifikat`** · samma `InfoPlist.loctable` (`certificate` = ”certifikat”) · `confirmed`.
- **(SSH) host key → bara `nyckel`** · engelskan säger medvetet ”key”, inte ”host key”, så svenskan gör likadant. ❌
  Skriv inte genitiv på `{host}`: värdet är okontrollerat och kan sluta på s-ljud (`nas`), där svenskan inte lägger till
  något. Skriv i stället `nyckeln från {host}` · `high` (konstruktionen), `tentative` (att `nyckel` räcker som term utan
  `värd`-led, men engelskan gör samma val).
- **compromised (om en återkallad nyckel) → `komprometterad`** · standardordet i svensk säkerhetstext; ingen
  förstahandskälla i de lästa paketen · `tentative`, låg risk. Ramen `är märkt som …` speglar `SecurityInterface` sv
  ”märks som betrott”.
- **sign-in method → `inloggningsmetod`** · byggt på katalogens satta `sign in → logga in`
  (`fileExplorer.network.signIn`, `.login.title`) · `high`.
- **Signed out (tillstånd på prickens knappbeskrivning) → `Utloggad.`** · samma rot som `logga in`/`logga ut`
  (`errors.provider.iCloud.serious`: ”Logga ut och in igen”). En-genus, så formen stämmer både mot `anslutningen` och
  mot `du` · `high`.
- **doesn't support yet → `stöder … inte än`** · katalogens `errors.git.bareRepo.title` (”Bare-repon stöds inte än”) och
  de många `stöder`-raderna i `errors.json` · `high`.
- **The connection dropped → `Anslutningen bröts`** · ordagrant katalogens egen `errors.listing.connectionDropped.title`
  · `high`. Cmdrs återanslutningsloop beskrivs med `arbetar på att`, som är katalogens register
  (`askCmdr.tool.unknown.doing` = ”Arbetar”); `jobbar` förekommer ingenstans.
- **Can't disconnect while operations are in progress →
  `Det går inte att koppla från medan åtgärder pågår på den här servern`** · exakt ramen från systerraden
  `fileExplorer.navigation.ejectBusyTooltip` (”Det går inte att mata ut medan åtgärder pågår på den här enheten”) ·
  `high`. ❗ Det här är knappbeskrivningen på en AVSTÄNGD knapp i panelvyn, inte ett menyalternativ, så den bär INTE
  menyernas ` (upptagen)`-markör (se style-guiden § Busy (disabled) menu items).
- **Forget server → `Glöm servern`; Forget saved password → `Glöm sparat lösenord`** · redan satta i `menu.network.*`
  och `fileExplorer.network.share.forgetPassword`; dialogrubrikerna ärver dem ordagrant så samma handling heter samma
  sak i meny och dialog · `high`.
- **drops the connection → `släpper anslutningen`; stops listing it → `slutar visa servern i listan`** · `släppa` är
  katalogens verb för att ge upp en anslutning (`reconnect.finalAttempt`, `unreachable.detailGaveUp`), och huvudordet
  skrivs ut eftersom både `anslutningen` och `servern` är en-genus och ett ensamt `den` skulle bli tvetydigt · `high`.
- **Your files stay on the server → `Dina filer ligger kvar på servern`** · `ligga kvar` är katalogens satta bild för
  det som blir orört (`trash.undonePartial`, `pane.directConnection*Toast`) · `high`.

värde, så ICU-dubbleringen `''` blir aldrig aktuell. Ingen ny termdrift heller: `Cancel`, `Try again`, `Disconnect`,
`Forget server` och `Forget saved password` återanvänder exakt de svenska formerna katalogen redan hade.

## Serverhubben: tabellen, statusarna och tomläget (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.serverPinnedToast`/`.serverUnpinnedToast`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

Volymväljarens gamla rad `Nätverk` heter nu `Servrar` och öppnar en tabell över varje sparad server (SFTP, WebDAV, SMB)
plus dem Cmdr ser i närheten, med kolumnerna Namn / Typ / Adress / Status / Senast använd och en `Lägg till server…`-rad
sist. GRUPPEN raden ligger i heter fortfarande `Nätverk` (`fileExplorer.navigation.groupNetwork`).

Belägget kommer från de LEVANDE macOS-paketen, inte från referenshögen: `_ignored/i18n/` finns inte på den här maskinen.
Den dokumenterade reservvägen är `docs/i18n/reference-pile/how-to-mine.md` § ”No pile on this machine?”. Allt nedan är
läst på macOS 26.6.2, build 25G83, 2026-09-06.

- **servers (plural) → `servrar`** · Finder `LocalizableMerged.strings` sv `SD13` = ”Anslutna servrar”; singularen
  `server` var redan satt i `style.md` · `high`. Gäller `fileExplorer.navigation.networkVolume`,
  `shortcuts.scope.servers`, `commands.serversShow.label` (”Visa servrar”) och `one`/`other`-grenarna i
  `servers.hub.rowCount` (`{countText} server` / `{countText} servrar`).
- **Places (rubrik för det som ligger UNDER en server) → `Platser`** · Finder sv `FI1` = ”Senaste platser” för Recent
  Places · `high`. Det är delade mappar på en SMB-värd i dag och hinkar hos en lagringstjänst sedan, så rubriken måste
  vara vidare än `delade mappar` (som den gamla `shortcuts.scope.places` sa).
- **Last used (kolumnrubrik) → `Senast använd`** · Finder sv `N228` = ”Senast öppnad” för Last Opened och `N169.32`
  ”Senast öppnad” för Date Last Opened; samma `Senast` + perfektparticip-mönster · `high`. En-genus (`servern`), alltså
  `använd`, inte `använt`.
- **Never (i kolumnen `Senast använd`) → `Aldrig`** · `Keychain Access.app` sv `KeychainFirstAid.loctable` `never` =
  ”aldrig” · `high`. Versal här bara för att det är cellens första ord.
- **Address (kolumnrubrik) → `Adress`** · Finder `ConnectToWindow.strings` `YEA-3L-WnW.placeholderString` skriver ut
  ”Serveradress”, men kolumnen står redan under rubriken `Servrar`, så förleden behövs inte · `high`. Samma kortform som
  anslutningsarkets fält (`servers.sheet.address`).
- **Type (kolumnrubrik) → `Typ`** · katalogens `queryUi.ai.filter.type` · `high`.
- **Status (kolumnrubrik) → `Status`, identiskt med engelskan** · katalogens `licensing.section.labelStatus` har redan
  formen med sin egen `sameAsSourceJustification`; `style.md` listar `Status` bland det som står kvar ordagrant ·
  `high`. Därav en `sameAsSourceJustification` även på `servers.hub.colStatus`.
- **Statusarna är en-genus particip, för de beskriver `servern`**: `Ansluten` (Finder sv `SD13` ”Anslutna servrar”),
  `Sparad`, `Hittad i närheten`, `Utloggad` · `high`. `Utloggad` var redan satt i förra passet (prickens
  knappbeskrivning); statuscellen tar samma ord utan punkt.
- **nearby → `i närheten`** · Finder sv `MR19`/`MR21` (”… dela med personer i närheten”) · `high`. ❌ Inte `upptäckt`,
  som katalogen bär för mDNS-fynd i nätverksbläddraren (`browser.cannotRemoveDiscovered`): engelskan väljer medvetet det
  enkla `found`, och `hittad` är den svenska motsvarigheten i samma register.
- **Waiting for you to check the key → `Väntar på att du ska kontrollera nyckeln`** · ramen `Väntar på att X ska VERB`
  är katalogens egen (`fileOperations.transferProgress.stallWaitingDestination`/`.stallWaitingSource`), och
  `kontrollera` är dess satta verb för att stämma av något (`errors.listing.connectionRefused.suggestion`) · `high`.
  `nyckel` utan `värd`-led enligt förra passets beslut om SSH host key.
- **local network discovery → `sökning i det lokala nätverket`** · katalogens `settings.network.firstTriggerDone.label`
  = ”Nätverkssökning startad” ger `sökning` som huvudord, och `lokalt nätverk` är macOS namn på behörigheten (se §
  Inställningar) · `high`. `is off` → `är avstängd` (en-genus, som `sökning`), exakt katalogens formel i
  `fileExplorer.navigation.driveIndex.tooltipIndexingOff` (”Enhetsindexering är avstängd i Inställningar”).
- **Turn it on in Settings → `Slå på den i Inställningar`** · samma par som `driveIndex.refusedIndexingOff` (”… Slå på
  den under Indexering > Enhetsindexering”) och `onboarding.stepAi.off.help` (”slå på det senare i Inställningar”);
  `den` syftar på `sökning`, en-genus · `high`.
- **pin / unpin (en server, i volymväljaren) → `Fäst / lossa`** · katalogens `menu.tab.pinTab`/`.unpinTab` (”Fäst
  flik”/”Lossa flik”) · `high`. Snedstrecket står kvar för att engelskan medvetet packar båda riktningarna i ETT
  kommando; katalogens `eller`-form (`commands.viewShowHidden.label`) hör till de strängar där engelskan själv skriver
  ”or”.
- **volume switcher → `volymväljaren`** · redan satt (`shortcuts.scope.volumeChooser`, `commands.volumeClose.label`) ·
  `high`. Fäst-toasterna säger `din volymväljare`, som engelskan.
- **”It's still saved” → `Servern är fortfarande sparad.`** · huvudordet skrivs ut i stället för `den`, samma skäl som
  förra passets `släpper anslutningen`-not: `volymväljaren` och `servern` är båda en-genus, så ett ensamt `den` blir
  tvetydigt · `high`.
- **Cmdr couldn't change where {name} shows → `Cmdr kunde inte ändra var {name} visas.`** · ramen ordagrant från
  systerraden `fileExplorer.navigation.disconnectRefusedToast` (”Cmdr kunde inte koppla från {name}.”) · `high`.
- **Add server… → `Lägg till server…`; Edit server… → `Redigera server…`; Disconnect server → `Koppla från server`** ·
  obestämd form utan artikel, som Finder sv `N84` ”Anslut till server…” och katalogens `commands.volumeSelect.label`
  (”Välj volym”); `Redigera` från `commands.fileEdit.label`, `Koppla från` från `menu.network.disconnect` · `high`.
- **NAS står kvar** · katalogen skriver redan `en NAS hemma` (`onboarding.stepOptional.networking.desc`) och
  `NAS-enheter` (`settings.network.smbConcurrency.description`); en-genus, så `en Mac eller NAS` delar artikel · `high`.

`Forget saved password` återanvänder ordagrant `menu.network.forgetSavedPassword` (”Glöm sparat lösenord”), så samma
handling heter samma sak i meny och kommandopalett. Ingen apostrof i något värde, så ICU-dubbleringen `''` blir aldrig
aktuell. Enda `sameAsSourceJustification` i passet är `servers.hub.colStatus`.

## Serverhubben: anslutningsarket, värdnyckeln och Gå till sökväg (`servers.sheet.*`, `servers.hostKey.*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

Ny yta: modalarket där man skriver in en server (SMB, SFTP, WebDAV), SSH-frågan om värdnyckeln inuti det, och två
förhandsrader under Gå till sökväg.

Belägget kommer från de LEVANDE macOS-paketen, inte från referenshögen: `_ignored/i18n/` finns inte på den här maskinen
(den är gitignorerad och ligger bara i en klon), och `docs/i18n/reference-pile/how-to-mine.md` § ”No pile on this
machine?” är den dokumenterade reservvägen. Allt Apple-belägg nedan är läst på macOS 26.6.2, build 25G83, 2026-09-07. ❗
`grep` hittar ingenting inuti en `.loctable`; strängarna togs ut med `plutil -convert json -o -`.

Rikaste källan för hela arket är Apples egen anslutningsdialog:
`NetAuthAgent.app/Contents/Resources/AuthDialog.loctable` och `.../Localizable.loctable`, plus Finders
`sv.lproj/ConnectToWindow.strings`.

- **Connecting… → `Ansluter…`** · ordagrant NetAuthAgent `Localizable.loctable` `CONNECTING_TO_GENERIC` (”Connecting…” =
  ”Ansluter…”) · `high`.
- **Protocol (som tillgänglighetsnamn på en väljare) → `Protokoll`** · `AddPrinter.app/…/IP.plugin/…/IP.loctable`
  `100257.ibExternalAccessibilityDescription` = ”Protokoll” (samma slags dolda namn på samma slags protokollväljare),
  och `100268.title` = ”Protokoll:” · `high`.
- **Browse… → `Bläddra…`** · Finder `sv.lproj/ConnectToWindow.strings` `48.title` = ”Bläddra”, alltså Bläddra-knappen i
  Apples Anslut till server-fönster · `high`. Punkterna står kvar, som i engelskan.
- **hostname → `värdnamn`** · Certificate Assistant `EvalCerts.loctable` `DSi-jV-fn4.title` = ”Värdnamn:”, Automator
  `Variables.loctable` ”Host name” = ”Värdnamn”, AddPrinter ”Enter host name or IP address.” = ”Ange värdnamn eller
  IP-adress.” · `high`. Passar katalogens redan satta `nätverksvärdar` (`commands.networkRefresh.label`).
- **passphrase → `lösenfras`** · Certificate Assistant `P12Password.loctable` `23.title` (”Enter Passphrase:” = ”Ange
  lösenfras:”) och `Security.framework/…/SecErrorMessages.loctable` (”Passphrase is required for import/export.” =
  ”Lösenfras krävs för import/export.”) · `high`. ❌ Inte `lösenordsfras`, som bara är OID-namnet i
  `Security.framework/…/OID.loctable`, alltså en fältetikett i ett certifikat och inte UI-språk.
- **fingerprint (på en nyckel eller ett certifikat) → `fingeravtryck`** · `Security.framework/…/Certificate.loctable`
  ”Fingerprints” = ”Fingeravtryck”, och ConfigurationProfilesUI `str_SCEP_InvalidFingerprint` = ”Fingeravtrycket för
  SCEP-servern ”%@” matchar inte.” · `high`. Neutrum: `ett fingeravtryck`, `fingeravtrycket`.
- **`Key passphrase` → `Nyckelns lösenfras`; `Key fingerprint` → `Nyckelns fingeravtryck`** · genitivform på båda, så de
  två `Key …`-etiketterna i samma ark läses som ett par. En sammansättning (`nyckellösenfras`) blir ogenomskinlig, och
  `Lösenfras för nyckeln` är för lång för en fältetikett bredvid `Namn` och `Adress` · `high`.
- **Sign in to X → `Logga in på X`** · Setup Assistant `ICLOUD_ONLY_LOGIN_TITLE` (”Sign In to iCloud” = ”Logga in på
  iCloud”) · `high`. Arkets rubrik blir alltså `Logga in på {name}`, utan citattecken kring namnet. Knappen `Sign in…`
  blir `Logga in…`, ordagrant ConfigurationProfilesUI `str_SignInToWorkOrSchoolAccount_Button`.
- **Signed out of X → `Utloggad från X`** · `Utloggad` var satt i hubbpasset; `från` är Apples preposition för
  riktningen (ConfigurationProfilesUI `str_BMAIDSignIn_Progress_SignOut` = ”Loggar ut från ”%@”…”) · `high`.
- **Trust (knapp) → `Lita på`, men objektet skrivs ut** · `SecurityInterface.framework/…/Localizable.loctable` `Trust` =
  ”Lita på” · `high`. ❗ Svenskan kan inte lämna partikelverbet naket som engelskans ”Trust and connect”, så knapparna
  heter `Lita på nyckeln och anslut` och `Lita på den nya nyckeln`. Samma skäl gör att `I''ve checked it` blir
  `Jag har kontrollerat nyckeln`: både `nyckeln` (en) och `fingeravtrycket` (ett) finns i rutan, så ett ensamt pronomen
  pekar åt två håll.
- **{host}''s key changed → `Nyckeln från {host} har ändrats`** · genitiv på `{host}` är förbjuden (okontrollerat värde,
  kan sluta på s-ljud), och `nyckeln från {host}` är redan katalogens form i `servers.refusal.hostKeyUntrusted`/
  `.hostKeyRevoked` · `high`.
- **reinstalled → `har installerats om`** · Apples verb är `installera om` (Problem Reporter: ”You may need to reinstall
  the application.” = ”Du kanske måste installera om appen.”, Erase Assistant: ”Reinstall macOS…” = ”Installera om
  macOS…”) · `high`.
- **Remote folder → `Mapp på servern`** · ❌ inte en `fjärr`-sammansättning. Apple har visserligen `fjärrinloggning`,
  `fjärrhantering` och `fjärrsynkronisering` (`CoreTypes.bundle/…/InfoPlist.loctable`, SSMenuAgent), men `fjärrmapp`
  finns inte i något läst paket och läser som jargong i en fältetikett. `@key` säger uttryckligen ”which folder ON THE
  SERVER”, och katalogen skriver redan `på servern` (`errors.listing.remotePermissionDenied.suggestion`,
  `fileExplorer.navigation.forgetServerConfirm`) · `high`.
- **Key file → `Nyckelfil`** · sammansättning av katalogens satta `nyckel` (`onboarding.cloudSetup.apiKeyAria` =
  ”API-nyckel”) och `fil`; ingen förstahandskälla har termen · `tentative`, låg risk (samma slags
  konventionssammansättning som `fillista`).
- **Reconnect (imperativ) → `Återanslut`** · FinanceKitUI `RECONNECT_ACCOUNTS_TITLE` (”Reconnect Your Existing Accounts”
  = ”Återanslut dina befintliga konton”) · `high`. `Reconnect automatically` följer Apples ordföljd verb + `automatiskt`
  (Dock: ”Automatically hide and show the Dock” = ”Göm och visa Dock automatiskt”), alltså `Återanslut automatiskt`.
- **How to connect (dolt gruppnamn) → `Hur du ansluter`** · engelskan väljer medvetet en fråga i stället för ett
  substantiv här, så svenskan gör samma sak; grannlegenden i samma ark heter däremot kort `Protokoll`
  (`servers.sheet.protocolLegend`). `du`-tilltalet är katalogens (style-guiden § Formality) · `high`.
- **Try the Nextcloud address → `Prova Nextcloud-adressen`** · `prova` är katalogens verb för att testa något
  (`errors.*`: ”Så här kan du prova”, `viewer.saveAs.*`: ”Prova en mindre markering?”), medan `Försök igen` är
  reserverat för `Try again`. Varumärket tar bindestreck i sammansättningen, som Apples `Time Machine-skiva` · `high`.
- **Cmdr stopped connecting to X → `Cmdr stoppade anslutningen till {name}`** · `stoppa` är katalogens verb för att
  avbryta något som pågår (`fileOperations.cancelRollback.*`: ”Cmdr stoppade det här på din begäran.”), medan `Avbryt`
  hör till knappen · `high`.
- **Opens X / Adds a server (förhandsrader under Gå till sökväg) → `Öppnar {name}` / `Lägger till en server`** · presens
  tredje person, som engelskan; obestämd artikel i den andra eftersom det är en ny server · `high`.
- **Remember in Keychain (kryssrutan) → `Kom ihåg i nyckelringen`** · `nyckelring` är BEHÅLLAREN, satt i § Serverhubben:
  anslutningsläget · `high`. `servers.sheet.needsStoredSecret` citerar etiketten ordagrant, så de två måste ändras ihop.

Återanvänt ordagrant från katalogen, så ingen ny termdrift uppstår: `Avbryt`, `Anslut` (`fileExplorer.network.connect`),
`Logga in` (`.signIn`), `Spara`, `Namn`, `Adress`, `Avancerat` (`settings.section.advanced`), `Användarnamn`,
`Lösenord`, `Anslut som gäst`, `Lägg till server` (`servers.hub.addServer` utan punkterna), `Anslut till server…`
(`settings.network.permissionIntroConnectLink`) och `Redigera` (`menu.bar.edit`).

Fyra `sameAsSourceJustification` i passet: `servers.sheet.protocolSmb`, `.protocolSftp`, `.protocolWebdav`
(protokollnamn som macOS sv själv skriver latinskt, ”WebDAV-lösenord”/”AFP-lösenord” i NetAuthAgent) och
`.addressPlaceholder` (`nas.local` är ett exempelvärdnamn med Bonjour-suffixet `.local`, identiskt på svenska). Ingen
apostrof i något värde, så ICU-dubbleringen `''` blir aldrig aktuell.

## Serverpanelen: återanslutningsloopen och den nyckelbaserade utloggningen (`servers.paneState.reconnecting`/`.signedOutNothingToAsk`)

Två rader i samma panelvy: rubriken medan Cmdr på egen hand försöker få tillbaka en tappad serveranslutning, och raden
som står i stället för `Logga in…`-knappen när servern bevisar sig med en nyckel och det alltså inte finns något fält
att fylla i. Ingen ny term behövde sättas: båda värdena är byggda av former katalogen redan hade. Apple-belägget nedan
är läst i de LEVANDE paketen på macOS 26.6.2, build 25G83, 2026-09-07 (`_ignored/i18n/` finns inte på den här maskinen;
reservvägen är `docs/i18n/reference-pile/how-to-mine.md` § ”No pile on this machine?”).

- **Reconnecting to X… → `Återansluter till {name}…`** · ramen är ordagrant katalogens egen
  `errors.listing.deviceReconnecting.title` (”Reconnecting to the device” = ”Återansluter till enheten”), och
  presens-formen speglar systerraden `servers.paneState.connecting` (”Ansluter till {name}…”, från Finder sv
  `LocalizableMerged.strings` `MN1` = ”Ansluter till ^0…”). Prepositionen `till` är Apples egen efter verbet: svensk
  Finder har åtta `återansluta till …`-strängar (`MT38.3`, `MT41_V1`/`_V2`, `MT44_*`, `NE111.1`) · `high`. Termen
  `återansluta` (presens `återansluter`) var redan satt i termbasen; den här nyckeln lägger bara till frasramen.
- **”This server signs in with a key rather than a password” →
  `Den här servern använder en nyckel i stället för ett lösenord`** · ramen `Den här servern använder …` är ordagrant
  systerraden `servers.refusal.authMethodUnsupported` (”Den här servern använder en inloggningsmetod som Cmdr inte
  stöder än”), som sitter i samma panelvy och svarar på samma fråga (vad servern vill ha) · `high`. ❌ Skriv inte
  `Den här servern loggar in med …`: på svenska blir servern då den som loggar in någonstans. Huvudordet är `nyckel`
  utan `värd`-led, enligt passet om SSH host key.
- **”so there''s nothing to type” → `så det finns inget att skriva`** · termbasens satta mall
  `”there''s nothing to …” → så det finns inget att …` (samma mall bär `errors.eject.volumeNotFound` och
  `.notAnSmbVolume`), och verbet är katalogens eget för att mata in text i ett fält: `ui.combobox.emptyText` säger
  ”Fortsätt skriva …” för ”Keep typing …” · `high`. ❌ Inte `Du behöver inte skriva något`: den formen hör till
  `errors.listing.deviceReconnecting.suggestion` (”Du behöver inte koppla ur något”), där engelskan har en fristående
  mening och inte ett `so there''s nothing to …`-led.
- **”Open it again to retry.” → `Öppna servern igen för att försöka på nytt.`** · formeln
  `<handling> igen för att försöka på nytt` är katalogens egen och står redan i ett dussin `errors.listing.*.suggestion`
  (”Gå hit igen för att försöka på nytt”) · `high`. Huvudordet skrivs ut i stället för `den`, precis som systerraden
  `servers.paneState.hostKeyChangedHint` (”… öppna servern igen för att kontrollera fingeravtrycket”): meningen före
  slutar på `nyckel` och `lösenord`, så ett ensamt `den` skulle peka åt fel håll.

Ingen `sameAsSourceJustification` i passet: båda värdena skiljer sig från engelskan. Ingen apostrof i något värde, så
ICU-dubbleringen `''` blir aldrig aktuell, och `{name}` står kvar oförändrad i den enda nyckel som bär den.

## Fästa servrar, betrodda värdnycklar och Android-raden i Inställningar (`menu.network.*`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`)

Tre ytor i samma pass: snabbmenyn på en serverrad i volymväljaren, engångsaviseringen som säger att `Nätverk`-gruppen
blivit lång, och två nya underavsnitt under Filsystem (`Servrar (SFTP, WebDAV)` med de betrodda värdnycklarna, och
`Android (ADB)` med statusraden för `adb`-kommandot).

Belägget kommer från de LEVANDE macOS-paketen, inte från referenshögen: `_ignored/i18n/` finns inte på den här maskinen
(den är gitignorerad och ligger bara i en klon), och `docs/i18n/reference-pile/how-to-mine.md` § ”No pile on this
machine?” är den dokumenterade reservvägen. Allt Apple-belägg nedan är läst på macOS 26.6.2, build 25G83, 2026-09-07. ❗
`grep` hittar ingenting inuti en `.loctable`; strängarna togs ut med `plutil -convert json -o -`.

- **Pin to switcher → `Fäst i volymväljaren`; Unpin → `Lossa`** · verben var satta i `menu.tab.pinTab`/`.unpinTab`
  (”Fäst flik”/”Lossa flik”) och i `commands.serversTogglePin.label` (”Fäst / lossa server”) · `high`. Asymmetrin är
  engelskans egen (`Pin to switcher` mot bara `Unpin`), så svenskan behåller den: `volymväljaren` skrivs ut i det
  fästande alternativet och utelämnas i det lossande, där raden i menyn redan svarar på ”ur vad”. ❌ Inte `Ta bort`, som
  katalogen reserverar för att plocka ut något ur en lista permanent; servern är kvar i listan Servrar.
- **”the Servers list” / ”your Network group” → `listan Servrar` / `Gruppen Nätverk`** · apposition, så UI-namnet står
  ordagrant som rubriken det pekar på (`fileExplorer.navigation.networkVolume` = ”Servrar”, `.groupNetwork` = ”Nätverk”)
  · `high`. En sammansättning (`Servrar-listan`, `Nätverksgruppen`) skulle tappa eller stava om rubriken. Engelskans
  possessiva `Your` faller bort: svenska aviseringsrubriker tar den inte, och gruppen är ändå användarens.
- **Got it → `Uppfattat`** · katalogens egen form på tre systerknappar (`ai.toast.gotIt`,
  `updates.moveToApplicationsDialog.gotIt`, `main.oldMacos.gotIt`) · `high`. macOS sv säger visserligen `OK` för ”Got
  It” (ShazamKitUI `SOUNDS_LIKE_ALERT_DISMISS`, DigitalTouchShared `INFO_DONE`, PodcastsFoundation
  `NOW_PLAYING_SCROLLING_TIP_DONE_BUTTON_TITLE`), men katalogen håller `OK` för dialogernas förvalsknapp
  (`ui.alertDialog.defaultButton`, `fileOperations.button.ok`), så `Uppfattat` bär den avfärdande nyansen engelskan
  valt.
- **trusted → `betrodd` (en) / `betrott` (ett) / `betrodda` (plural)** · Certificate Assistant sv
  `Localizable.loctable`: ”Trusted Root” = ”Betrodd rot”, ”Untrusted Root” = ”Ej betrodd rot”; ManagedClient
  `ConfigurationProfilesUI` ”listan över betrodda certifikat”; iCal-profilinsticket ”Certifikatet är inte betrott.” ·
  `high`. Därav `Betrodda värdnycklar` (kortrubrik), `Betrodd` (prefix före datumet, en-genus efter `nyckeln`) och
  `Inget betrott än.` (neutrum efter `inget`).
- **(SSH) host key → `värdnyckel`** · sammansättning av termbasens `värd`/`värdnamn` (Certificate Assistant
  `EvalCerts.loctable` `DSi-jV-fn4.title` = ”Värdnamn:”) och det satta `nyckel` · `tentative`, låg risk. Används bara
  där engelskan själv skriver ut `host key` (`settings.servers.card.trustedHostKeys`, `settings.summary.servers`); i
  serverhubbens texter, där engelskan säger bara `key`, står `nyckel` ensamt enligt passet 2026-09-06.
- **Not found → `Hittades inte`** · Tier 1 och entydigt: AppKit `FindPanel.loctable`, Foundation `URL.loctable`,
  CFNetwork, Dictionary `MainMenu.loctable` `100386.title`, AirPort Utility `placeholder.notfound`, Stickies `NOT_FOUND`
  · `high`.
- **Found at {path} → `Hittades: {path}`** · samma passivform som `Hittades inte`, så de två statusvärdena läses som ett
  par · `tentative` (kolonkonstruktionen är Cmdrs egen; inget läst paket har en `Found at`-motsvarighet). ❌ Inte
  `Hittades i {path}`: `{path}` är sökvägen till själva `adb`-kommandot, inte mappen det ligger i, så `i` skulle säga
  fel sak. Kolonet följer katalogens vana att låta ett värde stå efter ramen (style.md § ”Ett värde som hamnar efter
  kolon”).
- **Watching for phones → `Håller utkik efter telefoner.`** · Apple sv använder samma konstruktion i Notes
  `LearnMoreTagsWindow.loctable` `jRk-sf-TqB.title` (”Håll utkik efter taggförslag medan du skriver.”) · `high`.
  Negationen blir `Håller inte utkik efter telefoner just nu.` ❗ Inget om ADB-servern, prenumerationen eller socketen,
  precis som engelskan.
- **Re-check (knappen bredvid statusen) → `Leta igen`** · katalogens eget verb för exakt den här handlingen:
  `settings.fileOperations.adbBinaryPath.description` säger ”så letar Cmdr efter adb på vanligt sätt” · `high`.
  `Kontrollera igen` vore ordagrant men längre än knappen tål, och `leta` binder ihop knappen med platshållartexten
  `Leta efter adb på vanligt sätt`. Samma ord ordagrant i `settings.adb.install.intro` (”… och tryck sedan på Leta
  igen:”).
- **Browse… (knappen som öppnar filväljaren) → `Bläddra…`** · Finder sv `ConnectToWindow.strings` `48.title` =
  ”Bläddra”, och katalogens `servers.sheet.browse` har redan formen · `high`. ❗ Själva filväljarens OK-knapp heter
  `Välj` hos Apple (Finder sv `LocalizableMerged.strings` `NS0`/`NS1`/`NS2` = ”Välj fil”/”Välj mapp”/”Välj”), vilket är
  varför `settings.adb.pickerTitle` blir `Välj kommandot adb` medan knappen som öppnar den heter `Bläddra…`.
- **Tint server panes → `Tona serverpaneler`** · `Tona` var satt i systerraderna `settings.appearance.tintLocal.label`
  (”Tona paneler med lokala volymer”) och `.tintMtp.label` (”Tona MTP-paneler”) · `high`. Etiketten hette tidigare
  `Tona SMB-/nätverkspaneler`; engelskan namnger nu alla tre protokollen, så svenskan lyfter huvudordet till
  `serverpaneler` och låter parentesen bära `SMB, SFTP, WebDAV`. Beskrivningen skriver ut `en delad SMB-mapp` enligt
  termbasens `share → delad mapp`, och listan tar inget serie-komma (`…, en SFTP-server eller en WebDAV-server`).
- **USB debugging turned on → `har USB-felsökning aktiverad`** · ordagrant katalogens
  `settings.fileOperations.adbEnabled.description` · `high`. `Browse an Android phone` blir
  `Bläddra i en Android-telefon`: `bläddra i` är riktningen för att gå igenom ett innehåll, som Finder sv ”Bläddra bland
  tillgängliga servrar” (`ConnectToWindow.strings` `47.ibShadowedToolTip`).
- **De interna spårningsnycklarna följer sina syskon ordagrant** · `settings.behavior.serversPinHintSeen.label` =
  `Tips om lång Nätverk-grupp visat` speglar `settings.behavior.openTerminalHereToastSeen.label` (”Tips om ”Öppna
  terminal här” visat”), och `.description` = `Om engångstipset om att lossa servrar har visats.` speglar samma nyckels
  beskrivning · `high`. Bindestrecket i `Nätverk-grupp` är Apples mönster för egennamn i sammansättning
  (`Time Machine-skiva`), så rubriken `Nätverk` står kvar oförändrad.

Två `sameAsSourceJustification` i passet: `settings.section.adb` (”Android (ADB)” är två egennamn plus parentes, och
inget däremellan finns kvar att översätta) och `settings.adb.status.label` (`Status`, samma skäl som
`servers.hub.colStatus` och `licensing.section.labelStatus`). `settings.section.servers` blir däremot
`Servrar (SFTP, WebDAV)`, eftersom huvudordet böjs. Ingen apostrof i något värde, så ICU-dubbleringen `''` blir aldrig
aktuell, och `{command}`, `{host}` och `{path}` står oförändrade.

## Android över ADB: panelen, volymväljaren och tipsraden (`adb.*`, `settings.behavior.adbHintDismissed.*`)

Tre ytor: helpanelsmeddelandet där fillistan skulle ha stått när en Android-telefon inte går att öppna (`adb.connect.*`,
samma röst som `servers.refusal.*`/`servers.paneState.*`), inforutorna på telefonens rad i volymväljaren
(`adb.readiness.*`) och den tysta raden överst i en MTP-panel som erbjuder den fullständiga vägen in (`adb.hint.*`),
plus frånkopplingsknappen (`adb.disconnect*`).

Referenshögen saknas på den här maskinen (`_ignored/i18n/` finns bara i en klon), så beläggen kommer dels från de
LEVANDE macOS-paketen (macOS 26.6.2, build 25G83, läst 2026-09-07; `.loctable` läses med `plutil -convert json -o -`, ❗
`grep` hittar ingenting inuti dem), dels — för Androids egen vokabulär — från AOSP:s egna svenska översättningar.

- **USB debugging → `USB-felsökning`** · nu FÖRSTAHANDSBELAGT, inte längre `tentative`: AOSP
  `frameworks/base/packages/SettingsLib/res/values-sv/strings.xml`, `enable_adb` = ”USB-felsökning” (och
  `clear_adb_keys` = ”Återkalla åtkomst till USB-felsökning”, `enable_adb_wireless` = ”Trådlös felsökning”) · `high`.
  Det är ordagrant etiketten användaren ser i Utvecklaralternativ på sin telefon, vilket är hela poängen med nyckeln.
  Därför `high`, inte `tentative`; `en`-genus står kvar (”USB-felsökning aktiverad”).
- **Allow (knappen på Androids egen ”Allow USB debugging?”-ruta) → `Tillåt`** · AOSP
  `frameworks/base/packages/SystemUI/res/values-sv/strings.xml`, `usb_debugging_allow` = ”Tillåt” (rutans rubrik är ”Ska
  USB-felsökning tillåtas?”, `usb_debugging_title`) · `high`. Ordet citeras som egennamn, aldrig böjt till ett verb,
  precis som engelskan gör: `adb.readiness.waitingForAuthorization` = ”Väntar på att du trycker på Tillåt i telefonen”
  och `adb.connect.unauthorized` = ”Titta på din telefon och tryck på Tillåt.”, båda med tryckverbet från nästa punkt.
  ❌ Inte `Godkänn` eller `Acceptera` — ordet måste stämma tecken för tecken med knappen på telefonens skärm.
- **tap (på en telefonskärm) → `tryck på`** · Androids sv genomgående (`Tryck på …`) · `high`. Katalogens `klicka` hör
  till musen på Macen; telefonen får `tryck på`.
- **Android platform tools → `Android platform tools`, oböjt** · redan satt i
  `settings.fileOperations.adbEnabled.description` (”Kräver Android platform tools (kommandot ”adb”)”) · `high`. Googles
  egennamn på hämtningen; ingen genitiv-`s`, ingen bestämd form.
- **”The Android tools on this Mac” → `Android-verktygen på den här Macen`** · `Android-verktyg` är katalogens eget ord
  i samma beskrivning (”om du inte har några Android-verktyg installerade”), och `den här Macen` är den satta formen ·
  `high`. ❗ Ingen diagnostik: inget om `adb`-servern, transporten eller demonen, precis som engelskan.
- **”Cmdr couldn't find …” → `Cmdr kunde inte hitta …`** · katalogens egen formel
  (`errors.listing.notFound.explanation`, `licensing.error.shortCodeNotFound`) · `high`.
- **reseat the cable → `dra ur och sätt i kabeln igen`** · `dra ur` är det satta ordet för kabeln ur uttaget (style.md §
  `koppla från`/`koppla ur`/`dra ur`), och katalogen har redan konstruktionen i `errors.provider.macDroid.transient`
  (”Dra ur och anslut USB-kabeln igen”) · `high`. Ett ensamt `sätt i kabeln igen` skulle inte säga att den först ska ut.
- **Disconnect {name} (telefonens knapp) → `Koppla från {name}`** · ordagrant systernyckeln
  `fileExplorer.navigation.disconnectPlaceAriaLabel` för en server · `high`. De två SKA vara lika: engelskan valde
  `Disconnect` framför `Eject` av samma skäl på båda ytorna (inget görs säkert att dra ur), och `mata ut` är reserverat
  för utmatning. Motsvarande upptagen-inforuta tar systerformuleringen ordagrant från
  `fileExplorer.navigation.ejectBusyTooltip`, som har exakt samma engelska slut (”on this device”):
  `Det går inte att koppla från medan åtgärder pågår på den här enheten`.
- **Dismiss → `Avfärda`** · katalogens genomgående form på sju systernycklar (`downloads.empty.dismiss`,
  `crashReporter.dialog.dismiss`, `errorReporter.sentToast.dismiss`, `lowDiskSpace.toast.closeTooltip` med flera) ·
  `high`. Skärmläsarnamn på ×-knappen, alltså imperativ.

## Android över ADB i Inställningar (`settings.fileOperations.adb*`)

Evidence for these came from the LIVE macOS bundles, not the pile: `_ignored/i18n/` doesn't exist on every machine (it's
gitignored and only ever in one clone), and `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" is
the documented fallback. Anchored to macOS 26.6.2, build 25G83, read 2026-09-06.

- **debugging: `felsökning`** · Apple sv, Safari `sv.lproj/DeveloperPreferences.strings` ("Aktivera felsökningsläge för
  intelligent skydd mot spårning", "…för privat klickmätning"). `high`.
- **USB debugging (the Android developer-options switch): `USB-felsökning`** · AOSP's own Swedish, `SettingsLib`
  `values-sv/strings.xml` `enable_adb` = "USB-felsökning" (read 2026-09-07), which is verbatim what a Swedish Android
  phone shows in Utvecklaralternativ. `high`. Agreement is `en`-gender: "USB-felsökning aktiverad", never "aktiverat".
  The `Allow` button on the phone's own prompt is `Tillåt` (SystemUI `usb_debugging_allow`); see § Android över ADB:
  panelen, volymväljaren och tipsraden.
- **turned on (a switch on the phone): `aktiverad`** · matches the catalog's own phone-context wording (`errors.*`:
  "Kontrollera att USB-filöverföringsläge är aktiverat på telefonen") and the settled `enable → aktivera`. `high`.
- **"Location of adb" (a field holding a path to a binary): `Sökväg till adb`** · `path → sökväg` from `terms.json`,
  plus Finder sv `Toolbar.strings` `179/180.title` = "Sökväg" for Path. ❌ Not Finder's "Plats:" (its Get Info "Where:",
  `InfoWindowGeneralView.strings` `8C4-bd-mis.title`): `plats` names the enclosing folder, so it would read as "which
  folder", while the field wants the executable itself. `high`.
- **"Android file access over ADB" (switch label): `Åtkomst till Android-filer via ADB`** · `åtkomst` is the catalog's
  settled access noun (`errors.listing.*`, `fileExplorer.pane.connectedDirectlyToast`); `via` is what the catalog
  already uses for a transport ("via en USB-kabel", "via USB"). `high`.
- **`adb`, `ADB`, `Android SDK`, `Homebrew` kept verbatim** · the command and the packaged product names, which the `en`
  `@key` descriptions name as stay-as-is. `high`.
- **`platform tools` kept verbatim too, which the `en` description does NOT ask for** · that description asks for the
  phrase the way the vendor's localized Android renders it, and `vi` therefore translates it. Swedish keeps it because
  the only place a Swedish reader meets the phrase is Android Studio's SDK Manager, whose package list reads
  `Android SDK Platform-Tools` in every locale. `tentative`: no AOSP or Google Swedish string was found either way.
  `Android platform tools` takes no genitive `s` in Swedish ("Kräver Android platform tools"), the same reflex as the
  `macOS`-genitive note in style.md.
- **this Mac: `den här Macen`** · already the catalog's form (`settings.terminal.*`: "de terminalappar den hittar på den
  här Macen"; `main.*`: "den här Macen har en äldre version"). `high`.

## Dock-erbjudandet: engångsfrågan om att lägga Cmdr i Dock (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

En avisering som dyker upp några dagar in i användningen och frågar om Cmdr får lägga sig i Dock, plus de fyra korta
beskeden efter ett ja. Hela ytan pekar på macOS egna Dock, så macOS ordval vinner enligt style.md § ”Copy som pekar på
en systemyta stavas som macOS stavar den”. Belägg dels ur referenshögen (`_ignored/i18n/sv/`, läst 2026-09-09), dels ur
det LEVANDE systemet (macOS 26.6.2, build 25G83, läst 2026-09-09) där högen inte har Dock.app.

- **Dock → `Dock`, oböjt, utan artikel och utan possessiv** · Dock.app `sv.lproj/DockMenus.strings`: `KEEP_IN_DOCK` =
  ”Behåll i Dock”, `REMOVE_FROM_DOCK` = ”Ta bort från Dock”, `DOCK_SETTINGS` = ”Dock-inställningar…”; Finder
  `MenuBar.json` `300772.title` = ”Lägg till i Dock”; AppKit `Common.json` ”…hämtades från Dock” · `high`. Alltså
  `i Dock` / `från Dock`, aldrig `i Docken` eller `i din Dock`. Engelskans ”your Dock” tappar possessiven i svenskan,
  eftersom Apple aldrig sätter en. Sammansättningar bindestrecksbinds (`Dock-inställningar`, `Dock-erbjudande`).
- **Keep in Dock → `Behåll i Dock`; Remove from Dock → `Ta bort från Dock`** · Dock.app `DockMenus.strings`, ordagrant ·
  `high`. Det är parets kanoniska form, så `pin`/`unpin` på DENNA yta blir `behåll` / `ta bort`, ❌ inte katalogens
  `fäst` / `lossa`. De två hör till Cmdrs egna ytor (flikar, servrar; se § Inbyggda menyer och
  `commands.serversTogglePin.label`); Dock är Apples yta och tar Apples verb. `ta bort` krockar inte med style.md:s
  ”reservera `ta bort` för att plocka ur en lista” — en symbol ur Dock ÄR den betydelsen, inte radering från disk.
- **Add to Dock → `Lägg till i Dock`** · Finder `MenuBar.json` `300772.title` · `high`. Ja-knappen blir därför
  `Ja, lägg till i Dock` (fem ord, ryms på en knapp).
- **Applications folder → `mappen Appar`** · macOS 26 har bytt namn på mappen: Finder `Localizable.json`
  `Applications`/`Apps` → ”Appar”, `LocalizableMerged.json` `GROUP_APPLICATIONS` → ”Appar”, Finder `MenuBar.json`
  `258.title` → ”Appar”, AppKit `Menus.json` `Applications` → ”Appar” · `high`. ❌ Inte längre `Program`. AppKit
  `AppKitErrors.json` ger dessutom hela dra-meningen som modell: ”Försök med att dra ”%@” från papperskorgen till mappen
  Appar.” — alltså `dra … från mappen Appar`, med `mappen` framför namnet.
- **Finder → `Finder`, oböjt** · Finder `MenuBar.json` genomgående (”Nytt Finder-fönster”, ”Om Finder”, ”Avsluta
  Finder”) · `high`. Redan katalogens form.
- **configuration profile → `konfigurationsprofil`** · SystemSettings `InfoPlist.json`, ”Configuration Profile” =
  ”Konfigurationsprofil” · `high`. Gemen mitt i mening (svensk meningsversalisering).
- **managed by (en MDM-styrd inställning) → `styrs av`** · SystemSettings `Localizable.json` `MDMDisabledSettingsPane` =
  ”De här inställningarna styrs av en profil.” · `high`. Apples egen formel för exakt den här situationen. `hanteras av`
  sparas till personen som administrerar maskinen (”Den som hanterar den här Macen”), så engelskans avsiktliga
  upprepning managed/manages överlever som styrs/hanterar.
- **icon (appens symbol i Dock) → `symbol`** · katalogens satta ord (`useAppIconsForDocuments`: ”appsymboler”,
  ”filtypssymboler”; `terms.json` `badge`) och macOS Finder (”Öka symbolstorlek”, ”Som symboler”) · `high`. Genitiv på
  varumärket enligt style.md: `Cmdrs symbol` (konsonantslut tar naket `-s`).
- **the Dock didn't reload → `Dock startade inte om`** · Dock.app `DockMenus.strings` `RELAUNCH` = ”Starta om” · `high`.
  Det som faktiskt inte hände är att Dock-processen inte startade om. ❗ Meningen får ALDRIG säga att fästningen
  uteblev: symbolen ÄR på plats, bara omritningen saknas. Därför `Cmdrs symbol är på plats, men Dock startade inte om.`
  — påståendet om att den är på plats står först och är det som bär.
- **”No, thanks” → `Nej tack`** · svensk standardform, utan komma (Språkrådet) · `high`. ❌ Inte katalogens `Inte nu`
  (den borttagna askCmdr.consent.decline): den lovar en ny fråga senare, och Cmdr frågar aldrig igen efter det här
  nejet.
- **Engångsflaggan i Inställningar följer systerraderna** · `Erbjudande om Dock visat` speglar
  `Tips om lång Nätverk-grupp visat` och `Tips om USB-felsökning avfärdat` (obestämt huvudord + particip;
  `ett erbjudande` → neutrum → `visat`), och beskrivningen speglar `Om engångstipset om att lossa servrar har visats` ·
  `high`. `engångserbjudandet att lägga till …` utan andra `om`, för att slippa `om … om`.
- **”a few days” → `några dagar`, aldrig ett tal** · tröskeln kan flyttas, så en siffra skulle bli osann. Vag småmängd,
  precis som engelskan.
- **Turn on USB debugging → `Slå på USB-felsökning`** · `slå på` är katalogens och macOS sv:s verb för att slå på en
  funktion (”Slå på Wi-Fi/AirDrop/fildelning”; katalogen: `fileExplorer.navigation.driveIndex.menuEnable`,
  `servers.hub.discoveryOffLink`) · `high`. `Aktivera` är kvar för det som aktiveras en gång (en licens, en
  AI-leverantör), och `har USB-felsökning aktiverad` står kvar som TILLSTÅND i inställningsraderna.
- **How (länken som öppnar Androids egen instruktion) → `Hur?`** · `tentative`. Apple sv har ingen enordslänk för ”How”:
  en svepning av 7 851 `.loctable`-filer i `/System` + `/Applications` gav noll träffar på ett ensamt `Hur`, `Så här`
  eller `Så här gör du`. Apples enordsform för en dokumentationslänk är `Läs mer` (PassKit `LEARN_MORE_BUTTON_TITLE`
  m.fl.), men det är ”Learn more”, inte ”How”, och engelskan valde medvetet frågeordet. `Hur?` behåller den talspråkliga
  frågan som `@key`-beskrivningen ber om och håller sig till ett ord. Frågetecknet behövs: ett naket `Hur` läses som
  avhugget.
- **De två interna spårningsnycklarna följer sitt syskon ordagrant** · `settings.behavior.adbHintDismissed.label` =
  `Tips om USB-felsökning avfärdat` speglar `settings.behavior.serversPinHintSeen.label` (”Tips om lång Nätverk-grupp
  visat”) och `settings.behavior.openTerminalHereToastSeen.label`, och `.description` =
  `Om engångsraden som erbjuder USB-felsökning har avfärdats.` speglar
  `Om engångstipset om att lossa servrar har visats.` · `high`. `avfärdat`/`avfärdats` i neutrum efter `Tips` respektive
  opersonlig passiv, samma mönster som syskonens `visat`/`visats`.
- **`You stopped opening your phone.` → `Du stoppade öppnandet av din telefon.`** (`adb.connect.cancelled`) · `stoppa`,
  precis som systernyckeln `search.coverage.walk.cancelled` (`Du stoppade den här sökningen`) och
  `errors.volume.cancelled` (`Cmdr stoppade det här på din begäran.`) · `high`. ❌ Inte `avbröt`: `Avbryt` är KNAPPENS
  etikett (`fileOperations.button.cancel`), så meningen skulle läsas som en hänvisning till knappen. Verbalsubstantivet
  `öppnandet av` följer katalogens egen form (`skapandet av mappen`, `borttagningen av originalen`), och `öppna` är det
  satta verbet för en telefon (`adb.connect.waitingHint`).

Ingen `sameAsSourceJustification` i passet (`adb.volumeLabelWithSuffix` bar redan sin från en tidigare omgång). Ingen
apostrof i något värde, så ICU-dubbleringen `''` blir aldrig aktuell, och `{name}` står oförändrad i den enda nyckel som
bär den.

## Den låsta serveridentiteten (`servers.sheet.identityLocked`)

De två raderna under de gråade fälten `Adress` och `Användarnamn`, när användaren REDIGERAR en sparad server.

- **`the account` (fältet man loggar in på servern med) → `kontot`** · katalogen använder redan ordet i just den
  betydelsen (sex träffar i `errors.json`, en i `onboarding.json`) · `high`.
- **Hjälptexten namnger handlingarna exakt som knapparna den pekar på**: `glöm` från `menu.network.forgetServer` ("Glöm
  servern") och `lägg till` från `servers.sheet.addTitle` ("Lägg till server"). Ett synonymval ("ta bort", "skapa")
  skickar läsaren att leta efter en meny som inte finns.
- **`are what name this server` → `är det som identifierar den här servern`** · arket har ett eget fält `Namn`
  (`servers.sheet.name`), så meningen får inte bygga på "namnge": då låter det som om den handlade om den etiketten.
  `identifiera` säger det som avses (de två värdena ÄR servern) · `high`.

## Toasten när det inte fanns något sparat lösenord (`fileExplorer.navigation.forgetSecretNoneToast`)

- **`There was no saved password for {name}.` → `Det fanns inget sparat lösenord för {name}.`** · återanvänder
  `sparat lösenord` och `för {name}` ordagrant från de tre levererade syskonen (`menu.network.forgetSavedPassword` och
  `fileExplorer.navigation.forgetSecretConfirmTitle` = ”Glöm sparat lösenord”, `.forgetSecretConfirm`,
  `.forgetSecretRefusedToast`) · `high`.
- **`Det fanns …` är den svenska existenssatsen i preteritum**, samma konstruktion som
  `fileOperations.trash.undoUnavailable` (”Det finns inget att lägga tillbaka.”). Obestämd form (`sparat lösenord`)
  eftersom satsen nekar existensen; den bestämda formen `det sparade lösenordet` hör hemma i bekräftelsedialogen, där
  lösenordet faktiskt finns.
- Ingen ursäkt och inget `gick inte`: ingenting misslyckades, vilket är hela poängen med nyckeln.
- Referenssamlingen fanns inte på den här maskinen (`_ignored/i18n/` saknas även i huvudklonen), så beslutet vilar på
  den redan levererade katalogen och den här ordlistan.

## Återförsökens längd, rubriken om värdnyckeln och Androids Tillåt-knapp (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`/`.retryKeepsTrying`, `adb.readiness.waitingForAuthorization`)

- **`{seconds}`/`{minutes}` har nu ett ICU-plural med TVÅ platshållare** (`servers.paneState.retryTotalSeconds`,
  `.retryTotalMinutes`): `{seconds}` väljer bara grenen, det som läses är `{secondsText}`, det redan formaterade talet.
  Svenskan har `one` och `other` (CLDR, style.md § Plurals): `1 sekund` / `60 sekunder`, `1 minut` / `2 minuter`,
  ordagrant från `main.quit.countdown` och `indexing.eta.hoursMinutesLeft` · `high`.
- **Båda värdena är byggstenar i `servers.paneState.retryKeepsTrying`** (`Fortsätter försöka i totalt {duration}.`), så
  de står nakna, utan preposition och utan punkt.
- **`Cmdr won't connect to {name}` → `Cmdr ansluter inte till {name}`** · presens, precis som systernyckeln
  `servers.refusal.hostKeyRevoked` (`Cmdr ansluter inte till servern.`). Engelskan gick från ”stopped connecting” till
  en stående vägran, och preteritum (”stoppade anslutningen”) lät som ett avbrutet försök · `high`.
- **`Allow` är Androids egen knapp → `Tillåt`**, ordagrant från `adb.connect.unauthorized`
  (`Titta på din telefon och tryck på Tillåt.`), utan citattecken precis som där, så att texten och skärmen visar samma
  ord. I `adb.readiness.waitingForAuthorization` står `i telefonen`, inte `på telefonen`, för att slippa tre `på` i
  samma korta mening · `high`.
- Referenssamlingen fanns inte på den här maskinen (`_ignored/i18n/` saknas även i huvudklonen), så beslutet vilar på
  den redan levererade katalogen och den här ordlistan.

## Serverradens kontextmeny: `Öppna` och `Redigera server…` (`menu.network.open`, `menu.network.edit`)

- **`Open` (på en serverrad) → `Öppna`** (`menu.network.open`) · ordagrant samma som `menu.file.open`, för det är samma
  betydelse: att gå in i något, inte att lämna en fil till en app. Svenskan skiljer inte på de två, och det gör inte
  macOS heller: Finder använder samma verb i `Öppna` (`LocalizableMerged` `N151`), `Öppna med` (`N152`) och
  `Öppna i nytt fönster` (`FV7`, gå-in-i-betydelsen) (Finder 26.6.2, build 25G83, läst 2026-09-07) · `high`.
- **`Edit server…` → `Redigera server…`** (`menu.network.edit`), kopierat byte för byte från
  `commands.serversEdit.label` · `high`. Båda öppnar samma blad; två olika etiketter skulle läsas som två funktioner.
  Uttrycksprickarna är det ENA tecknet `…` (U+2026) och ska stå kvar. Obestämd form här, till skillnad från systern
  `menu.network.forgetServer` (`Glöm servern`), eftersom `commands.serversEdit.label` redan är levererad så.
- **Båda likheterna bevakas av en check**, de är inte bara snygga: `i18n-terms` larmar när två nycklar med samma
  engelska värde går isär på svenska. Skriver du om den ena måste partnern med.
- **`menu.*` är en RAW-familj**: Rust ritar menyn via `menu_t`, aldrig via `t()`. Apostrofer förblir ENKLA och en
  dubblerad `''` fäller `i18n-icu`. Ingen av de två värdena har någon.
- Referenssamlingen saknas på den här maskinen, men `Finder.app` ger samma Tier 1-belägg direkt ur systemet
  (`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?").

## Funktionstangentsradens snabbmeny (`menu.context.hideFunctionKeyBar`, `fileExplorer.functionKeyBar.hiddenToast`)

- function key bar (raden med funktionstangent-kommandoknappar längst ned i fönstret) → funktionstangentsraden · redan
  fastställt i katalogen (`settings.appearance.showFunctionKeyBar.label`); återanvänt för snabbmenyalternativet och
  tillhörande meddelande · high

## Ask Cmdr namnger bara chattpanelen (`askCmdr.wake.needsFullDiskAccess`, `settings.ai.tooltipOff`, `settings.ai.provider.description`, `settings.askCmdr.proactive.description`, `ai.cloudConsent.askCmdr.proactive`)

Engelskan behåller nu `Ask Cmdr` bara där namnet pekar på själva CHATTPANELEN (dess rubrik, alternativet i Visa-menyn,
palettkommandot, inställningsavsnittet, av/på-reglaget). Varje mening som bara beskrev vad AI:n gör bytte subjekt: de
flesta säger `Cmdr`, ett fåtal säger `the AI`. Det är engelskan i nyckeln som bestämmer, aldrig minnet av hur nyckeln
såg ut förut.

- the AI (satsens subjekt) · **AI:n** · katalogens egen form i `ai.translateError.*` (`AI:n tog för lång tid`,
  `AI:n kom tillbaka tom`, genitiven `AI:ns svar`) · `high`
- AI features · **AI-funktioner**, obestämd form · `settings.ai.tooltipOff` (`AI-funktioner är avstängda.`) och
  `settings.ai.provider.description` (`Välj hur AI-funktioner drivs.`) · `high`
- chat (panelen, substantiv) · **chatt** · Microsoft sv terminology (`chat` → `chatt`, verbet → `chatta`), och katalogen
  har redan `chatten` och `chattar` · `high`
- **`chatt` och `samtal` är inte utbytbara: var och en följer sin egen nyckels engelska.** `chat` är `chatt`
  (`startar en chatt`, `settings.askCmdr.proactive.description`), `conversation` är `samtal` (`startar ett samtal`,
  `ai.cloudConsent.askCmdr.proactive`). Engelskan skiljer på dem i grannycklar, så svenskan gör det också.
- provider (av AI) · **leverantör** · Microsoft sv terminology listar både `leverantör` och `provider`; katalogen kör
  `AI-leverantör` genomgående, och `leverantör` är det ord Apple-svenskan skulle välja · `high`
- **Andra meningen i `askCmdr.wake.needsFullDiskAccess` kopierar `search.coverage.setUpFullDiskAccess`**
  (`Ställ in full skivtillgång`), eftersom `@key` kräver att de två ytorna säger samma sak. Det blir
  `Klicka för att ställa in full skivtillgång.` · `high`.
- **`Cmdr` tar plain `-s` i genitiv** (`Cmdrs anteckningar`), enligt § "Genitiv på ett varumärke" i `style.md`.

## Stegen för att sätta upp en AI-leverantör (`onboarding.cloudSetup.*`)

Nycklarna `onboarding.cloudSetup.*`, granskade mot referenssamlingen (`sv/microsoft-terminology/`).

- download (verb) · **hämta** · `onboarding.cloudSetup.step.install` var katalogens ENDA `Ladda ner` mot 43 `hämta`, och
  är nu `Hämta och installera`. Apple sv säger `Hämta` (`Hämtade filer`, `Hämtningar`), och macOS vinner över Microsofts
  delade `ladda ned` / `nedladdning` / `hämtning` (termvalsprincip 2) · `high`
- placeholder (exempeltexten i ett fält) · **platshållare** · Microsoft sv terminology · `high`
- deployment (i Azure) · **distribution** · Microsoft sv terminology (`deployment` → `distribution`, tre poster) ·
  `high`
- endpoint · **slutpunkt** · Microsoft sv terminology (fem poster), och rubriken strax ovanför legenden är redan
  `Slutpunkts-URL` (`onboarding.cloudSetup.step.endpoint`) · `high`
- `Ollama`, `LM Studio`, `Azure OpenAI`, `Azure`, `api-version` och kommandot `ollama pull llama3.2` står kvar
  ordagrant.

## Dockmenyn (`menu.dock.*`)

Menyn som dyker upp när man högerklickar på Cmdrs symbol i Dock. Belagd i macOS egen dockmeny och i Finders menyrad,
inte i referenssamlingen: `_ignored/i18n/sv/` bär Finder, AppKit och Systeminställningar, men INTE Dock. Källan är
`/System/Library/CoreServices/Dock.app/Contents/Resources/sv.lproj/DockMenus.strings`, läst med
`plutil -convert json -o -` (macOS 26.6.2, build 25G83, läst 2026-09-09).

- **`Open <app>` i Dock · `Öppna <app>`, utan citattecken** · Dock sv `OPEN` = `Öppna`, och namnformerna `HIDE_NAME` =
  `Göm %@` / `SHOW_NAME` = `Visa %@` sätter appnamnet naket. FILNAMNSformen är en annan sträng, `OPEN_FILENAME` =
  `Öppna ”%@”`, som lägger svenska citattecken runt namnet. Cmdr är ett appnamn, alltså `Öppna Cmdr`. ❌ Skriv inte
  `Öppna ”Cmdr”`: det är filnamnsformen och läses som att man öppnar en fil som heter Cmdr. `high`.
- **`Go to Folder…` · `Gå till mapp…`** · Finder sv `MenuBar.json` (Gå-menyn) och fönstertiteln i `GotoWindow.json`
  (`Gå till mapp`). Obestämd form utan artikel, versal bara på första ordet. `high`.
- **`Connect to Server…` · `Anslut till server…`** · Finder sv `MenuBar.json` (Gå-menyn) och `ConnectToWindow.json`
  (`Anslut till server`). Stämmer med `style.md`s settlade server-post. `high`.
- **`Search files…` · `Sök filer…`** · samma kommando som `menu.edit.searchFiles` i menyraden, som redan säger
  `Sök filer…`; `@key` säger uttryckligen att de två ska formuleras lika. `high`.
- **`{name} ({parent})` står oförändrad** · rent skiljetecken runt två filnamn. Svenskan hänger på ett förtydligande
  inom parentes i samma ordning som engelskan: Finder sv `SB_iCloudDetail` är den nakna `^0 (^1)`
  (`macOS/Finder/LocalizableMerged.json`), och katalogen gör redan likadant (`Mata ut ({name}) (upptagen)`). Nyckeln bär
  en `sameAsSourceJustification`. `high`.

`Dock` böjs inte och tar ingen artikel (`i Dock`), enligt § Dock-erbjudandet ovan.

## Erbjudandet om ”Visa i Finder” och aviseringen vid första träffen (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

Två ögonblick i samma funktion: engångsfrågan om ”Visa i Finder” från andra appar får öppnas i Cmdr, och
engångsaviseringen första gången en sådan begäran landar här. Båda ytorna pekar på macOS egna kommandon, så macOS ordval
vinner enligt style.md § ”Copy som pekar på en systemyta stavas som macOS stavar den”.

- **”Show in Finder” → `”Visa i Finder”`, med svenska citattecken** · Redan satt i
  `settings.navigationAndFileOps.card.showInFinder` och `settings.revealHandler.description` · `high`. Aviseringarna
  återanvänder exakt den formen, så att inställningskortet och aviseringen kallar samma sak samma sak.
- **pane → `panel`** · Katalogens form i `fileExplorer.doubleClickHint.body` (”panelens bakgrund”) · `high`.
- **Settings (Cmdrs eget fönster) → `Inställningar`** · `settings.window.title` · `high`.
- **”for a while now” → `ett tag nu`** · Avsiktligt vagt: tröskeln kan flyttas, så ❌ aldrig en siffra. Samma regel som
  `main.dockPinNudge.body` (”i några dagar nu”) · `high`.
- **Aviseringen vid första träffen är ❌ ingen ursäkt** · Den säger vad som just hände, varför, och var reglaget finns.
  Därav `Cmdr är inställd på att fånga upp dem`, ❌ aldrig ”tyvärr” · `high`.

## Introduktionsguidens omskrivning: checklistan, AI-steget och sammanfattningarna

De 23 nyckarna i `onboarding.json` som steg 1–4 fick när guiden skrevs om: kryssrutelistan i beta-steget, AI-stegets
långa förklaringar och enradssammanfattningarna bredvid strömbrytarna i valfria steget.

### Kryssrutelistan i beta-steget (`onboarding.stepBeta.checklist.*`)

- **checklist · `checklista`** · Microsoft sv terminology (`checklist` → `checklista`, term-id 30962 → 1113079, plus två
  `Checklist` → `Checklista`-poster). `high`. Rubriken heter `Checklista för att komma igång`, eftersom katalogen redan
  ramar in guiden som `komma igång` (`onboarding.wizard.title` = ”Kom igång med Cmdr”, `onboarding.wizard.progressLabel`
  = ”Förlopp för att komma igång”), inte som `introduktion`.
- **`each takes 30 seconds` behöver ett huvudord: `varje punkt tar 30 sekunder`** · engelskans nakna `each` dinglar på
  svenska på samma sätt som ett räknat `*Text`-tal utan substantiv gör (`style.md` § Plurals). Punkten i en checklista
  är `punkt`. `high`.
- **`star` (GitHub-verbet) · `stjärnmärk`** · GitHubs eget gränssnitt ger INGEN svensk form att kopiera: GitHub
  lokaliserade sitt UI till bland annat svenska 2010–2016 men lade ner det 2016-11-18, och gränssnittet är sedan dess
  enbart engelskt (`Preferred spoken language` styr bara enstaka kommunikationsytor). Så valet faller på katalogens egen
  precedens, `onboarding.stepBeta.checklist.star` (”Stjärnmärk repot på GitHub”), med Microsoft sv (`star` → `stjärna`,
  fem poster) som stöd för substantivet `stjärnor`. `high`.
- **`repo` · `repot`** · katalogens egen form i `onboarding.stepBeta.checklist.star`. Låneordet böjs som ett neutrumord
  (`ett repo`, `repot`, `repon`). Skiljt från `git-repository` i `settings.json`, som är den tekniska helformen. `high`.
- **`Like` (AlternativeTos knapp) · `Gilla`** · AlternativeTo har inget svenskt gränssnitt (sajten är helt engelsk och
  knappen räknar `likes`, kontrollerat 2026-09-09), så verbet översätts: Microsoft sv terminology ger `Like` → `Gilla` i
  sex poster. `high`. `AlternativeTo` står kvar ordagrant som sajtnamn.
- **`title` på en webbsida · `rubriken`** · `Cmdr`-rubriken högst upp på AlternativeTo-sidan är en sidrubrik, inte en
  fönstertitel. Svenska citattecken runt namnet (`rubriken ”Cmdr”`) per `style.md`. `high`.
- **`notice` (om folk och sökmotorer) · `upptäcka`** · naturlig svenska för att få syn på något nytt; `lägga märke till`
  är längre utan att bli tydligare. `tentative` (sammansatt av vardagsspråket, ingen direkt källa).
- **`Save` (knappen bredvid e-postfältet) · `Spara`** · macOS AppKit `SavePanel.json` (`Save` → `Spara`),
  `Document.json` (samma). `high`. Samma ord citeras i `signup.rejected` och `signup.unreachable`, som pekar på just den
  knappen.
- **`Email address saved` (kryssmarkeringens skärmläsarnamn) · `E-postadressen sparad`** · `email address` →
  `e-postadress` från Microsoft sv terminology; `e-postadress` är en-genus, så participet blir `sparad`, aldrig
  `sparat`. Bestämd form eftersom raden pekar på just den adress användaren skrev in. `high`.
- **`<field></field>` mitt i meningen** · taggen är tom och renderar in-/utmatningsrutan plus Spara-knappen inne i
  satsen, så den måste stå där svenskan vill ha objektet: `Ange din e-postadress <field></field>, så hör jag av mig …`.
  Kommat före konsekutivt `så` står kvar (`style.md`). Första person, som resten av beta-steget, och samma ”hör jag av
  mig”-formel som `onboarding.stepBeta.emailNote` redan använder. `high`.

### Registreringen till e-postlistan (`onboarding.stepBeta.signup.*`)

- **`mailing list` · `e-postlistan`** · Microsofts `distributionslista` är Exchanges distributionslista (en adressgrupp
  i en organisation), inte en prenumerationslista, alltså fel betydelse på samma sätt som `aktie` för `share` och
  `redigera` för `redact`. `sändlista` finns inte i referenssamlingen alls. `e-postlista` är den genomskinliga
  vardagssvenskan och håller ihop med `.unreachable`s ”du står inte på listan än”. `tentative` (MS-betydelsen förkastad,
  ingen förstahandskälla).
- **`typo` · `stavfel`** · macOS AppKit `Accessibility.json` (`Misspelled` → `Felstavat`) ger roten; `stavfel` är
  standardformen av substantivet. `high`. ❌ Total Commanders `Skrivfel!` (`WCMD.LNG` 625/1245/1907) är en FALSK VÄN:
  den sitter bland filoperationsfelen och är `Write error`, inte `typo`. Använd den inte som belägg.
- **`sign up` · `registrera`; `signup server` · `registreringsservern`** · Microsoft sv terminology (`sign up` →
  `registrera` / `registrera sig`), och katalogen säger redan ”vi kunde inte registrera dig just nu”
  (`onboarding.stepBeta.signup.failure`). `high`.
- **Sökvägen i `signup.unreachable` skrivs `Inställningar › Uppdateringar och integritet`** · båda halvorna tas från
  katalogen själv, inte från en nyöversättning: `settings.section.updatesAndPrivacy` = ”Uppdateringar och integritet”
  och `settings` = `Inställningar` (`style.md`). Tecknet `›` står kvar ordagrant. `high`.

### AI-steget (`onboarding.stepAi.*`)

- **`dumber` · `dummare`** · engelskan är avsiktligt rättfram enligt `@key`, och svenskan har samma raka ord. Ingen
  mildring till ”mindre kapabel”. `high`.
- **`custom` (egen LLM) · `anpassad`** · katalogens settlade form i nio `settings.*`-nycklar (`Custom…` → `Anpassat…`,
  `Custom timeout` → `Anpassad tidsgräns`). `LLM` är en-genus (`en modell`), alltså `din egen anpassade LLM`. `high`.
- **`Configure it below` · `Konfigurera det nedan`** · `konfigurera` per arkivets `Konfigurera…`
  (`fileExplorer.archiveEnterMenu.configure`). `high`.
- **`<strong>`-blocket i `stepAi.local.tooltip` MÅSTE vara ordagrant `onboarding.stepAi.cloud.label`** · texten pekar
  användaren på alternativknappen längre ner, så de två är en enhet på samma sätt som ett `*Aria`-par: båda säger
  `Ja, jag vill ha AI`. Skrivs den ena om måste den andra skrivas om samtidigt.
- **`<em>och</em>` bär betoningen ensam** · engelskan betonar `and` just för att markera att det INTE brukar gå att få
  båda. Lägg därför inte till `både` framför: `både … <em>och</em> …` gör betoningen överflödig och tar udden av
  meningen. `high`.
- **`ignore this and go on` · `strunta i det här och gå vidare`** · `strunta i` matchar appens informella röst;
  `bortse från` är byråkratiskt. Etiketten i `{nextLabel}` ramas in av svenska citattecken (`”{nextLabel}”`), inte
  engelska. `high`.
- **`badge` · `märke`** · Microsoft sv terminology (`badge` → `märke`; den andra posten, `aktivitetsikon`, är
  ikonräknaren på en appsymbol och fel sak här). `<alpha></alpha>-märken` sätter taggen först och hänger på
  sammansättningsledet med bindestreck. `high`.
- **`work-in-progress areas` · `de områden som är mest under arbete`** · `under arbete` är den etablerade svenskan för
  work in progress; `pågående` ensamt säger bara att något rör sig, inte att det är ofärdigt. `tentative` (ingen direkt
  UI-källa).
- **`helps me fix bugs` · `hjälper mig att fixa buggar`** · `fixa` för `fix`, matchar katalogens ledigare register i
  `onboarding.stepBeta.openBeta`. Den nästan likalydande syskonmeningen med `spot bugs` (`hitta buggar`) satt i
  feedbackkanalslistans intro och försvann med omskrivningen av steg 3. `high`.

### Enradssammanfattningarna i valfria steget (`onboarding.stepOptional.*`, `settings.revealHandler.notProductionBuild`)

Alla fyra sitter bredvid en strömbrytare och får inte radbrytas, så de hålls på ungefär engelskans längd och lånar
terminologin från syskonnyckelns `…desc`.

- **`Local network access` · `Lokalt nätverk`** · Apples egen etikett för behörigheten, läst live i
  `/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex/…/Localizable.loctable` (`LOCAL_NETWORK` =
  `Lokalt nätverk`, `LOCAL_NETWORK_SUMMARY` = ”Tillåt att apparna nedan får hitta och kommunicera med enheter i det
  lokala nätverket.”, macOS 26.6.2 build 25G83, läst 2026-09-09); referenssamlingens `macOS/`-bunt bär den inte, så
  detta är den dokumenterade live-fallbacken. `high`. ❌ Inte den beskrivande `Lokal nätverksåtkomst`: det är samma
  feltyp som `fullständig åtkomst till skivan` mot Apples `Full skivtillgång`, alltså ett namn användaren inte hittar i
  Integritet och säkerhet. `onboarding.stepOptional.networking.desc` är rättad till samma etikett, så syskonnycklarna
  citerar nu Apples namn ordagrant båda två; engelskan citerar `Local Network` av samma skäl.
- **`speeds up searches` · `snabbar upp sökningar`** · `sökning` per `style.md`s search-post. `high`.
- **`check` (uppdateringskontrollen) · `kontroll`** · syskonnyckeln `onboarding.stepOptional.updates.desc` säger redan
  `licenskontroller` för samma sorts anrop. `high`.
- **`native handler` · `macOS inbyggda hanterare`** · `inbyggd` är dokumentets settlade ord för Apples egna ytor
  (`style.md` § ”Inbyggda menyer följer Finders ordval”), och `macOS` slutar på s-ljud och tar därför varken genitiv-`s`
  eller apostrof (`macOS inbyggda hanterare`, precis som `macOS inbyggda SMB-anslutning`). `high`.
- **`suppresses` · `håller tillbaka`** · syskonnyckeln `onboarding.stepOptional.mtp.desc` säger redan ”måste hålla
  tillbaka den macOS-processen”. `high`. Skilt från `style.md`s regel att `Suppress` i visa/göm-betydelsen blir `Dölj`:
  här handlar det om att stoppa en process, inte om att gömma något.
- **`and the such` · `och liknande`** · engelskans avsiktligt slarviga vändning; `och liknande` är den vardagliga
  svenska motsvarigheten och används redan i `onboarding.stepOptional.networking.desc` (”en NAS hemma och liknande”).
  `high`.
- **released copy / Dev and test builds · `släppt version` / `utvecklings- och testversioner`**
  (`settings.revealHandler.notProductionBuild`) · Microsoft terminology (`SWEDISH.tbx`: `build` → `version`, `release` →
  `släppa` / `version`, `production build` → `produktionsversion`). `version` och inte `kopia`: meningen handlar om en
  utgåva, som `commands.appCheckForUpdates.description`, inte om en andra körande process som
  `main.instanceLock.alertBody`. Andra meningen följer `settings.revealHandler.notInApplications`
  (`varje klick på ”Visa i Finder” pekar på ingenting`). `high`.

## Förhandsvisningen hämtar filen först (`viewer.pull.*`, `viewer.error.stoppedResponding`)

ICU-värden. Panelen syns mitt i förhandsvisningen när Cmdr kopierar en fil från en telefon, en server eller ett arkiv
till en temporär fil och det tar mer än en sekund.

- **fetch (kopiera en fil hit innan den visas): `hämta` / `hämtning`** · macOS Systeminställningar i referenssamlingen,
  `sv/macOS/SystemSettings/Localizable.json` nyckeln `Fetching Menu Item` (`Fetching…` = `Hämtar…`), läst 2026-09-10;
  samma ord som termbasens `download`-post. `high`. Rubriken följer Finders förloppsfönster (`LocalizableMerged.strings`
  `PW5_V1` = `Förbereder kopiering av ”^1”`, `PW45.2` = `Förbereder delning av ”^0”`, macOS 26.6.2 build 25G83, läst
  2026-09-10): verb i presens och filnamnet inom `”…”`, som katalogens övriga meningar med ett filnamn
  (`menu.context.copyNamed`). `för förhandsvisning` är ett substantiv utan pronomen, så inget `den`/`det` behöver stämma
  med filnamnet.
- **x of y (mängd av hela storleken): `{doneText} av {totalText}`** · Finder `PW3` (`^0 of ^1 – ^2` = `^0 av ^1 – ^2`,
  kopieringsfönstrets storleksrad) och `PW8` (`Kopierat: ^0 av ^1`), macOS 26.6.2 build 25G83; Thunar `%s av %s`;
  katalogens `fileExplorer.imageIndex.folder.someIndexed`. `high`.
- **so far (efter en storlek): `hittills`** · termbasens `so far`-post och `queryUi.results.live.matchesSoFar`. `high`.
- **”This file stopped arriving” · `Hämtningen av den här filen står stilla.`** · `står stilla` är katalogens satta ord
  för en överföring som inte rör sig (§ Överföringen som står stilla, valt framför `har stannat`), och `Hämtningen`
  knyter an till rubrikens `Hämtar`. Andra meningen följer `settings.mediaIndex.clip.failed`
  (`Kontrollera din anslutning och försök igen.`) och `errors.listing.couldntReadUnknown.suggestion`
  (`fortfarande är ansluten`), utan komma före `och` mellan två korta satser (`style.md`). `ansluten` stämmer med
  `telefonen` och `servern`, båda en-genus. `high`.

## Inaktuellt index på en telefon över ADB (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

Telefonversionerna av den gula statusens tre texter. En Android-telefon över ADB säger aldrig till när filer ändras, så
indexet kan vara inaktuellt fast telefonen sitter i. ❗ Inget om frånkoppling, precis som engelskan. Syskonen
(`tooltipStale`, `staleDialog.title`/`.body`) ger ramen: rubriken är ordagrant systerns med `telefonens` i stället för
`enhetens`, och `filer på den`, `Den gula statusen bredvid …`, `mappstorlekar och sökresultat` och `en ny genomsökning`
är systerdialogens egna ord.

- **phone (en Android-telefon som volym): `telefon`** · Microsoft-terminologin i referenssamlingen (`phone` → `telefon`,
  läst 2026-09-11) och katalogens satta ord i hela `adb.*` (`Din telefon är inte ansluten längre.`). En-genus:
  `telefonens index`, `filer på den`. Termbasens MTP-post `enhet` gäller den generiska enheten; när engelskan säger
  `phone` står `telefon`. `high`.
- **keep (an index) current: `hålla … uppdaterat`** · katalogens `tooltipFresh` = `Indexerad och uppdaterad`, så färsk
  och inaktuell status delar ord; neutrum efter `indexet`. `high`.
- **changes Cmdr makes / changes made on the phone: `sina egna ändringar` / `de ändringar Cmdr själv gör` /
  `ändringar som görs på (själva) telefonen`** · passivt `görs` med flit: ändringen kan komma från användaren eller från
  en annan app, så inget subjekt passar. `high`.
- **reminder: `påminnelse`** · Microsoft-terminologin (`reminder` → `påminnelse`, läst 2026-09-11).
  `stays as a reminder` → `finns kvar som en påminnelse`. `high`.

aldrig aktuell.

## Rotmapp och startmapp för en sparad server (`servers.sheet.rootFolder*`, `servers.sheet.startFolder*`, `servers.sheet.nameHelp`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`)

ICU-värden. Arket för att lägga till eller redigera en SFTP- eller WebDAV-server. Rotmappen är taket på servern (Cmdr
går aldrig ovanför den), startmappen är där panelen öppnas och måste ligga i rotmappen. Båda termerna står likadant i
etiketter, hjälprader och vägranden. Ersätter den borttagna etiketten "Remote folder" (`Mapp på servern`).

- **root folder (en sparad servers tak): `rotmapp`** · samma ord som termbasens `rotmapp`-post (§ Namnbyten och
  volymsvar): Microsoft-terminologin har `root folder`/`root directory` → `rotmapp` (term 313442, läst 2026-09-11),
  Thunar `Rotmappen har ingen förälder`, Total Commander `Gå till rotmappen`. Här är det inte filsystemets rot utan en
  mapp användaren väljer, precis som i engelskan; hjälpraden säger vad den gör. `high`.
- **start folder: `startmapp`** · ingen källa har begreppet som substantiv. Total Commander (`&Starta i:`) och Dolphin
  (utgången post `Start in:` → `Starta i:`) har bara verbetiketten, och nyckeln kräver en substantivfras. Sammansatt
  parallellt med `rotmapp` och Apples `start`-sammansättningar (`Startskiva`, `Startobjekt och tillägg`). Microsofts
  Windows-mapp `Startup` krockar inte (ingen `startup folder`-post i terminologin). `tentative`.
- **account: `konto`** · Microsoft-terminologin (`account` → `konto`, flera poster, läst 2026-09-11) och katalogens
  `servers.sheet.identityLocked` (`Adressen och kontot`). `high`.
- **Leave it empty (under ett fält): `Lämna fältet tomt`, inte `Lämna den tom`** · efter `Mappen …` skulle `den` syfta
  på mappen, och `en tom mapp` betyder en mapp utan filer. `fältet` är entydigt; namnfältets hjälprad säger likadant så
  att arkets hjälprader delar ram. Katalogens `Lämna det tomt, så …`
  (`settings.fileOperations.adbBinaryPath.description`) gäller där inget substantiv står före. `high`.
- **your account can read it: `ditt konto får läsa den`** · `får` för behörighet, inte `kan` (förmåga); katalogens
  `errors.listing.remotePermissionDenied.explanation` (`kontot du anslöt med har inte behörighet att öppna den`) bär
  samma betydelse i längre form. De två `*NotFound`-syskonen delar ram tecken för tecken (`style.md` § Syskonvarianter).
  `high`.
- **nothing was saved: `så Cmdr sparade ingenting`** · aktiv form i stället för `ingenting sparades` (`style.md`), komma
  före konsekutivt `så`. Första satsen är ordagrant `servers.refusal.timedOut` (`{host} svarade inte i tid`), sista är
  katalogens `Försök igen om en stund.` `high`.

## Varför en delad mapp inte monteras eller listan inte läses in (`errors.mount.*`, `errors.shareList.*`)

Meningarna under ”Det gick inte att montera den delade mappen” (`fileExplorer.networkMount.mountFailedTitle`) och ”Det
gick inte att ansluta till {hostName}” (`fileExplorer.network.share.connectFailedTitle`), plus notiserna
`fileExplorer.pane.directConnectionShareGoneToast`, `fileExplorer.pane.directConnectionMountNotRespondingToast`,
`fileExplorer.pane.directConnectionNotNetworkShareToast` och `servers.refusal.accountNotPermitted`. Tier 1 ur den
INSTALLERADE bunten `NetAuthAgent.app/Contents/Resources/Localizable.loctable` (macOS 26.6.2, 25G83, 2026-09-11), som
formulerar just de här fallen för ”Anslut till server” och saknas i referenssamlingen.

- **share → `delad mapp`** · termbasens satta ord och titeln bredvid · `high`. NetAuthAgent `EINFO_NO_SHARE` säger
  `Delningspunkten`, men katalogen har redan `delad mapp`.
- **guests → `gäster`** · NetAuthAgent `EINFO_NO_ACCESS_GUEST` (”Den här filservern tillåter inte gäståtkomst.”) ·
  `high`
- **reach → `nå`**, **didn't answer in time → `svarade inte i tid`**, **isn't responding → `svarar inte`** ·
  `servers.refusal.unreachable`, `servers.refusal.timedOut`, `adb.readiness.offline` · `high`
- **turned on → `påslagen`**, **same network → `på samma nätverk`** · `errors.listing.hostUnreachable.suggestion` ·
  `high`. **this computer → `den här datorn`**: meningarna visas även på Linux.
- **network shares → `delade mappar på nätverket`** · `errors.listing.remotePermissionDenied.explanation` · `high`
- **package → `paket`** · KDE Dolphin (”Kunde inte hitta paketet %1.”) · `high`. **distribution (Linux) →
  `distribution`** · inget belägg i Linux-betydelsen · `tentative`. `smbclient` är ett program:
  `som inte är installerat`, `Installera det`.
- **`igen … igen` undviks**: `resolutionFailed` säger `försök på nytt när servern är online igen` · `high`
- **there's nothing to speed up → `så det finns inget att snabba upp`** · ramen från `errors.eject.notAnSmbVolume` ·
  `high`
- Samma engelska, samma svenska: `errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` / `errors.shareList.authFailed`.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `textredigerare`** · Microsofts terminologi säger `textredigeringsprogram`; katalogen har redan
  stammen `redigerare` (`commands.fileEdit.label` "Redigera i standardredigeraren", `menu.file.edit` "Öppna i
  redigeraren"), och den gemensamma stammen avgör · `tentative`. Appslaget, ❌ inte Apples app TextEdit, vars namn
  kommer i `{app}`.
- **default text editor → `standardredigerare`** · katalogen (`commands.fileEdit.label`) · `high`.
- **Edit files in [app] → `Redigera filer i`** · meningen fortsätter i menyn · `tentative`.
- `{app}` efter `i`, utan böjning. Dismiss och Open settings samma som `commands.handler.openTerminalHere.dismiss` /
  `commands.handler.openTerminalHere.openSettings`.
- **system default → `Systemets standard`** · katalogen (`settings.appearance.language.opt.system`,
  `settings.appearance.dateTimeFormat.opt.system`) · `high`. `settings.behavior.textEditorApp.systemDefault` sätter
  appnamnet inom parentes efter, som `settings.appearance.language.opt.systemWithLanguage`.
- ”Choose an app…” och ”Checking your apps…” samma som `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking`. Tipset (`fileExplorer.edit.hint`) följer
  `commands.handler.openTerminalHere.hint`, utan platsen i Inställningar: knappen leder dit direkt.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected (utmatning eller avmontering pågår) → `håller på att kopplas från`** · passiv form av det
  satta `koppla från`, med `håller på att` för det pågående förloppet som Thunar `sv` uttrycker med presens (”Avmonterar
  enhet” / ”Matar ut enhet”) · `high`. ”Left its index as it was” → ”lät indexet vara orört”; ”try again in a moment” →
  ”försök igen om en stund”, som `errors.eject.notResponding`.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected (redan skett, enheten är borta) → `kopplades från`** · preteritum av det satta `koppla från`, och
  `indexing.staleDialog.body` säger samma sak om samma läge (”Medan {name} var frånkopplad”) · `high`. Alltså INTE den
  pågående formen ”håller på att kopplas från” i `fileExplorer.navigation.driveIndex.driveLeaving`. ”Starts from
  scratch” → ”startar från början”, som `indexing.rescan.incompletePreviousScan`; `genomsökning` och `mappstorlekar`
  kommer från samma familj.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.*`)

De fyra sidobestämda varianterna (`errors.write.deviceDisconnected.sided.destination.copy`,
`errors.write.deviceDisconnected.sided.destination.move`, `errors.write.deviceDisconnected.sided.source.copy`,
`errors.write.deviceDisconnected.sided.source.move`) står i samma dialog som
`errors.write.deviceDisconnected.message.copy` och får inte säga emot den. Läsaren har just ryckt ut en enhet mitt i en
överföring, så varje mening slutar med VAR filerna finns, inte med vad som hände.

- **was disconnected (redan skett) → `kopplades från`** · preteritum, som `indexing.needsFreshScan.afterDisconnect` och
  katalogens egen `errors.write.deviceDisconnected.message.copy` (”Enheten kopplades från under kopieringen”) · `high`.
  ❌ Inte den pågående formen `håller på att kopplas från` i `fileExplorer.navigation.driveIndex.driveLeaving`: här är
  enheten redan borta.
- **originals → `original`, bestämt `originalen`** · katalogen (`fileOperations.transferProgress.titleRemovingOriginals`
  ”Tar bort originalen...”, `fileOperations.cancelRollback.moveAlreadyLanded`), och macOS Finder `sv` har `originalet` i
  alias- och papperskorgssträngarna (”originalet ligger i papperskorgen”, verifierat i pilen 2026-09-16) · `high`.
- **untouched → `orörd`** · katalogens `errors.write.destinationNotFound.message.copy` (”Originalen är orörda”) och
  `errors.write.notConnected.message.destination` (”Dina filer är orörda”), plus macOS AppKit `Document` (”lämna filen
  orörd och jobba med en kopia”) · `high`.
- **so nothing is lost → `så ingenting har gått förlorat`** · macOS AppKit `Document` (”Ändringarna går förlorade om du
  inte sparar dem”) och Thunar `sv` (”så går den förlorad permanent”) · `high`. Perfekt, inte presens: enheten är redan
  ute, och `har gått förlorat` säger att läget står fast i stället för att varna för något som kan hända.
- **the rest → `resten`, still on the drive → `ligger kvar på enheten`** ·
  `fileOperations.cancelRollback.stoppedDeleting` säger ordagrant ”Resten ligger kvar.”, och `ligger kvar` är katalogens
  ord för det som blev stående (`fileExplorer.navigation.forgetServerConfirm`) · `high`. `enhet` är termbasens satta ord
  för drive.
- **{done} of {total} files → `{done} av {total} filer`** · `av` genomgående i katalogen (`viewer.pull.progress`,
  `fileExplorer.imageIndex.folder.someIndexed`) och i macOS Finders AirDrop-förlopp (”64,0 MB av 1,33 GB”) · `high`.
  Båda är färdigformaterade strängar: ingen ICU-siffersyntax runt dem.
- **”to it” → `dit`, och `{counterpart}` tar bara preposition** · ett riktningsadverb slipper gissa om enhetsnamnet är
  en- eller ett-genus, och `på {counterpart}` / `till {counterpart}` står utan artikel och utan böjning av samma skäl ·
  `high`.
- **before Cmdr could finish the move → `innan Cmdr hann göra klart flytten`** · `hann` är katalogens ord för det som
  inte blev av i tid (`errors.write.destinationFull.message` ”innan allt hann skrivas”,
  `queryUi.results.live.incomplete` ”Cmdr hann inte klart”) · `high`.
- **Försäkran i presens, platsen i preteritum**: ”Your originals are untouched where they were” blir ”Dina original är
  orörda där de låg”. Presens säger hur det ÄR nu, vilket är det lugnande, och `där de låg` pekar ut platsen utan att
  hänga på en relativsats som skjuter beskedet till slutet av meningen.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

Dialogen som visas när Cmdr inte kunde få bekräftat att de flyttade filerna landade, och därför behöll originalen.
Ingenting gick sönder, och ingen variant får läsas som att flytten gick fel.

- **couldn't confirm → `det gick inte att bekräfta` (rubrik), `Cmdr kunde inte bekräfta` (brödtext)** · katalogens satta
  formel för just det här läget: `fileExplorer.pane.trashUnconfirmedToast`, `fileExplorer.rename.unconfirmed` och
  `fileOperations.mkdir.timeoutMessage` säger alla ”Det gick inte att bekräfta att …” · `high`. Rubriken följer
  systernyckeln `errors.write.destinationNotFound.title` (”Det gick inte att hitta målmappen”), brödtexten behåller
  subjektet `Cmdr` eftersom engelskan har det och katalogen skriver aktivt (`settings.askCmdr.memory.notAllForgotten`
  ”Cmdr kunde inte radera alla anteckningar”). ❌ Inget `fel` och inget `misslyckades`: `kunde inte bekräfta` är
  bokstavligt, filerna kan mycket väl ligga där.
- **the move (substantiv) → `flytten`** · `errors.write.deviceDisconnected.message.move` (”Enheten kopplades från under
  flytten”) · `high`.
- **were saved on {volumeName} → `hade sparats på {volumeName}`** · pluskvamperfekt för det som skulle ha hunnit ske
  före bekräftelsen; `spara` är katalogens verb för att lägga undan data (`settings.advanced.logLlmCalls.description`) ·
  `high`. ❌ Inte `skrevs till`: `skriva` bär i katalogen själva överföringen som pågår
  (`fileOperations.transferProgress.rollbackTooltip`, `errors.write.writeError.message`), och poängen här är att filerna
  ska ha landat.
- **at the destination → `på målet`, men `målmappen` när man ska titta in i den** · `målet` är katalogens satta ord
  (`errors.write.destinationExists.message`, `errors.write.writeError.message`), och
  `errors.write.destinationNotFound.title` har `målmappen` om samma yta · `high`. `Titta i målet` går inte att läsa, så
  `errors.write.moveNotConfirmed.suggestion` tar mappformen.
- **it kept your originals where they were → `så dina original ligger kvar där de låg`** · omskrivet till presens ·
  `high`. `så Cmdr lät originalen ligga kvar` hade krävt ett andra `Cmdr` i meningen (se § Utmatning och frånkoppling:
  `Cmdr` upprepas inte i andra satsen), och presensformen svarar dessutom på frågan läsaren faktiskt har: var ligger
  mina filer nu?
- **Your originals haven't moved → `Dina original är orörda`** · `har inte flyttats` är den passiv-`-s` som `style.md`
  avråder från, och den hade upprepat brödtextens sista sats ordagrant två rader ned i samma panel. `är orörda` är
  katalogens egen försäkran (`errors.write.destinationNotFound.message.copy`) och varierar formuleringen precis som
  engelskan gör · `high`.
- **Have a look at the destination, then try … again → `Titta i målmappen och försök sedan flytta igen`** · `titta` är
  katalogens verb (`errors.write.newDataKeptAt.suggestion` ”Öppna {keptAt} och titta på den”), och `sedan` markerar
  ordningen utan komma mellan de två leden, som i § Utmatning och frånkoppling · `high`. Bara ett `igen`:
  `igen … på nytt`-formeln behövs först när engelskan upprepar ”again”.

## Arbetsmappen som blev kvar efter en flytt (`fileOperations.leftovers.stagingFolderKept`)

En informationstoast när en enhet ansluts igen (eller när Cmdr startar) och Cmdr hittar arbetsmappen från en flytt som
aldrig blev klar, med filer kvar i. Cmdr låter varje fil ligga, eftersom de kan vara personens enda exemplar. Ingenting
begärs av läsaren och ingenting står på spel: raden finns för att filer som saknas ska ha en plats att hittas på. ❌
Aldrig ett förslag om att radera mappen. Skilj den från `### cancelRollback.stagedLeftover.*`: där är resten Cmdrs egen
arbetsfil, här är det användarens filer som Cmdr medvetet skyddar.

- **unfinished (om själva flytten) → `avbruten`** · katalogens `settings.advanced.showStagingTempFiles.description`
  säger redan ”Rester från en avbruten kopiering” om exakt samma arbetsmapp, och Total Commander `sv` har `avbruten` för
  ett förlopp som inte blev klart (`555` ”Återta avbruten nedladdning”, `1233` ”Återuppta avbruten överföring”) ·
  `high`. Valet stod mot `ofullständig`, som `cancelRollback.stagedLeftover.*` satte för `unfinished copy`: det ordet
  beskriver ett halvfärdigt FÖREMÅL (en fil), medan `avbruten` beskriver ett förlopp som tog slut i förtid, vilket är
  det engelskan menar här. Båda är gångbara; gränsen går mellan sak och händelse.
- **the move (substantiv) → `flytten`, obestämt `en … flytt`** · redan satt i § A move that could not be confirmed ·
  `high`.
- **left them in place → `lät dem ligga kvar`** · `operationLog.rollback.partiallyRolledBackNotice` har exakt samma ram
  (”Cmdr ångrade det som gick och lät resten ligga kvar”), och `lät` bär att Cmdr VALDE att inte röra filerna, vilket är
  hela poängen · `high`. ❌ Inte `blev kvar` (som `cancelRollback.stagedLeftover.*`): det läses som en rest ingen tog
  hand om, och tar bort försäkran engelskan lägger i ”left them in place”.
- **a hidden folder named {folderName} → `en dold mapp som heter {folderName}`** · `dold` är termbasens satta adjektiv
  för `hidden` (se § Samma begrepp med olika engelska), `mapp` är en-genus så obestämd form blir `en dold mapp` ·
  `high`. `som heter` är katalogens egen formel för `named {x}` (`errors.mount.shareNotFound`,
  `errors.write.duplicateSourceNames.message`); macOS `sv` säger `med namnet` om samma sak (”Skapa en mapp med namnet
  ${fileName} inuti ${target}”) och hade fungerat lika bra, men katalogformen är talspråkligare och står redan på två
  ställen. Båda lämnar platshållaren oböjd, vilket är det som avgör.
- **Platshållarknepet: `på {volumeName}` och `som heter {folderName}`, båda utan artikel och utan böjning** · samma
  lösning som § A drive pulled mid-transfer (`på {counterpart}`, `hade sparats på {volumeName}`) · `high`. Enhetsnamnet
  är okontrollerad text i vilken skrift som helst, så ingen bestämd form, ingen genitiv och ingen kongruens får hänga på
  det. `{folderName}` är alltid `.cmdr-staging-<uuid>`, och `som heter` bär namnet som ett citat i stället för som ett
  led i satsen.
- **Kommat före `i en dold mapp`** står kvar från engelskan: det är en efterställd bestämning, samma rytm som
  `cancelRollback.stagedLeftover.named` (”… {name}, en ofullständig kopia som blev kvar …”) i samma fil. Det är alltså
  inget komma mellan två huvudsatser och krockar inte med `style.md` § Notes and decisions.
- ICU-fil, men värdet innehåller ingen apostrof, så dubbleringen `''` blir aldrig aktuell. Inget
  `sameAsSourceJustification`: värdet skiljer sig från engelskan.

## Favoritmenyn: ⌃D-ytan och dess tio strängar (`commands.favorites*`, `fileExplorer.navigation.favorites*`, `fileExplorer.navigation.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

⌃D fäller ut användarens bokmärkta mappar som en meny över den fokuserade panelen. De nio första raderna bär
siffertangenterna 1–9, sista raden är märkt `0` och lägger till panelens aktuella mapp. Volymväljarens tidigare
favoritavdelning är borta och ersatt av en enda topprad som byter ut växlaren mot menyn.

- **favorites menu: `favoritmeny` (rubrik), `favoritmenyn` (löptext)** · katalogen har redan `favoriter`/`favorit`
  (macOS Finder ”Favoriter”), och macOS `sv` bildar sammansättningen med `favorit-` som förled (”Favoritservrar:” i
  Finders anslutningsfönster). `high`. **Gränsen mellan formerna:** `shortcuts.scope.favoritesMenu` är en
  avdelningsrubrik och står därför obestämt, `Favoritmeny`, precis som sina grannar `shortcuts.scope.volumeChooser`
  (`Volymväljare`), `shortcuts.scope.fileList` (`Fillista`) och `shortcuts.scope.commandPalette` (`Kommandopalett`). I
  en mening är ytan däremot ett bestämt föremål, så `commands.favoritesOpen.description` säger `Öppna favoritmenyn …`.
  Den svenska bestämdhetsfällan (`../../guides/i18n-translation.md` § An `*Aria` key must contain its visible label)
  slår inte till här: ingen `*Aria`-nyckel citerar rubriken, så formerna får skilja sig. Skulle en sådan nyckel
  tillkomma är det rubriken som ska byta form, inte meningen.
- **Show favorites: `Visa favoriter`** · `menu.go.showServers` (”Show servers”) är redan `Visa servrar`, och macOS `sv`
  har ett dussin `Visa …`-alternativ i samma imperativform (”Visa sidofältet”, ”Visa förhandsvisning”). Gäller båda
  tvillingarna: `commands.favoritesOpen.label` och den nativa `menu.go.showFavorites`, som ska läsa likadant. `high`.
- **See {count} favorites: `Visa {count} favoriter`, alltså samma verb som `Show`** · engelskan växlar mellan `See` och
  `Show` för samma handling, svenskan gör det inte: `Visa` är det satta ordet för att fälla ut en lista, och katalogen
  säger redan `Visa licensinformation` (`commands.appLicenseKey.seeDetails.label`) och `Visa hela ändringsloggen`
  (`whatsNew.dialog.seeFullChangelog`) för engelskans `See`. `Se` är reserverat för att titta på något
  (`commands.helpWhatsNew.description`: ”Se vad som ändrats”). `high`. Följden är att `=0`-grenen blir teckenidentisk
  med `commands.favoritesOpen.label`; det är rätt, det ÄR samma handling.
- **Pluralformen i `fileExplorer.navigation.seeFavorites`: `=0` + `one` + `other`** · svenskans CLDR-kategorier är `one`
  och `other` (`new Intl.PluralRules('sv')`, se `style.md` § Plurals), och `=0`-grenen behålls för att engelskan bär
  den: den säger ”du har inga än” utan att skriva ut nollan, så `{count}` får inte stå där. `favorit` är en-genus och
  böjs `favorit`/`favoriter`. `high`.
- **current folder: `aktuell mapp` i etikett, `den aktuella mappen` i löptext** · `queryUi.scope.currentFolder` är redan
  `Aktuell mapp` och `queryUi.scope.useCurrentFolder` är `Använd aktuell mapp`, medan `commands.editPaste.description`
  skriver ut `i den aktuella mappen`. Total Commander `sv` har samma artikellösa etikettform (`Lägg till aktuell mapp`).
  `high`. **`0`-raden har numera EN enda nyckel**, `fileExplorer.navigation.favoritesAddCurrent`
  (`Lägg till aktuell mapp i favoriter`, etikettformen utan artikel). Genvägslistan CITERAR den raden för att förklara
  tangenten `0` i stället för att beskriva om den, så den läser samma värde · `high`. ❌ Hitta inte på en andra, bestämd
  formulering för listan. Skillnaden som består är mot det riktiga kommandot `commands.favoritesAdd.label`
  (`Lägg till i favoriter`), vilket engelskan avser.
- **mounted share: `monterad delad mapp`** · `fileExplorer.network.browser.noMountedShares` är redan
  `Inga monterade delade mappar från {hostName}`, och `settings.network.permissionWithout` säger
  `delade mappar som redan är monterade`. `delad mapp` är den satta termen för share (`terms.json` `network-share`).
  `high`. Inga protokollnamn i `fileExplorer.navigation.favoritesCantAddHere`, precis som engelskan undviker dem.
- **a disk: `en disk`** · katalogen skiljer redan på `disk` (engelskans `disk`: ”en intern disk”, ”en extern disk”, ”På
  disk”) och `enhet` (engelskans `drive`). `skiva` är reserverat för när Finders egen ordalydelse speglas (`style.md` §
  Terminology). `high`.
- **`favoritesCantAddHere` är ett påstående om DEN HÄR mappen, med skälet efter kolonet** ·
  `Den här mappen kan inte bli en favorit: favoriter fungerar bara på diskar och monterade delade mappar` · `high`.
  Samma subjekt som systerraden `fileExplorer.navigation.favoritesAlreadyAdded` (`Den här mappen är redan en favorit`),
  så de två gråade raderna läses som ett par. ❌ Inte längre `peka på`: det tvingade fram ett rektionsval för målet och
  gjorde raden klart längre än engelskan. `fungera på` är en platt lokativ och slipper dessutom den delade prepositionen
  (`på en disk` mot `i en delad mapp`), så plural i båda leden räcker. Ingen punkt på slutet (verktygstips).
- **`commands.favoritesAdd.description` nämner inte längre växlarens favoritavdelning**, som M3 tog bort, utan vart
  mappen faktiskt hamnar:
  `Lägg till den fokuserade panelens aktuella mapp i favoriter, så tar favoritmenyn dig tillbaka dit när som helst.` ·
  `high`.
- **siffertangenterna 1–9: `siffra`, inte `nummer`** · tangenterna är bokstavligen siffror, och `nummer` är i katalogen
  ett löpnummer på något (`viewer.statusBar.badge.indexedTooltip`: `radnumren`). Därav `Öppna favoriten med den siffran`
  (`commands.favoritesOpenByNumber.label`) och `tryck på en siffra` i `commands.favoritesOpen.description`. `tentative`:
  ingen källa i referenssamlingen namnger just den här ytan, men risken är låg och båda nycklarna använder samma ord.
- **`press a number to jump to that favorite` får sitt mål utskrivet** · ett naket `för att gå` går inte i svenskan, och
  engelskan stannade förut vid `to go`, så den här översättningen fyllde själv i ett vagt `dit`. Nu namnger engelskan
  målet, och svenskan gör detsamma: `…tryck på en siffra för att gå till den favoriten.` Samma reflex som katalogens
  `errors.listing.notAFolder.suggestion`-familj, där ”Gå hit igen” alltid får ut sitt mål. `high`.
- Inget komma före `och` i `commands.favoritesOpen.description`: två korta huvudsatser, enligt `style.md` § Notes and
  decisions. Engelskan har kommat, svenskan inte.
- Inga apostrofer i något av de tio värdena, så varken ICU-dubbleringen `''` eller RAW-familjens raka apostrof blir
  aktuell. Inget `sameAsSourceJustification`: alla tio skiljer sig från engelskan.

## Utmatningen som vägrades av en namngiven app (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

Sex nya värden i samma toast som § Utmatning och frånkoppling, alltså fortfarande EFTER kolon i
`fileExplorer.pane.ejectFailedToast` och RAW (vanliga apostrofer, `{app}`/`{apps}` som rena ersättningsmål). De tre
`unmountRefusedBy*`-nycklarna för appar är syskon till `unmountRefused`, så de delar dess ram ordagrant:
`… använder fortfarande den här enheten` + `… och mata sedan ut igen`. Bara subjektet och objektet byts ut.

- **`{app}` står först i meningen, obestämt och oböjt** · `unmountRefused` har redan `Något` i den positionen, så
  platshållaren ärver en färdig subjektsplats: `{app} använder fortfarande den här enheten.` Svenskans V2-ordföljd tar
  ett naket främmande namn utan artikel, och ingenting i satsen kongruerar med det · `high`. macOS AppKit `sv` säger
  samma sak passivt med citattecken (`The disk could not be ejected because it is in use by ”%@”` → ”Skivan kunde inte
  matas ut eftersom den används av ”%@””, läst i pilen 2026-09-16), vilket belägger `används` om ett namngivet program
  men inte formen: `style.md` avråder från passiv-`-s` när en aktiv sats finns, och nyckelns `@key` förbjuder
  citattecken runt namnet. Aktiv sats vinner alltså på båda punkterna.
- **”Close anything it has open there” → `Stäng allt den har öppet där`** · `allt` slipper `det den`-stammandet och
  `öppet` kongruerar med neutrum singular; `ha något öppet` är katalogens egen konstruktion
  (`errors.listing.deletePending.suggestion` ”appar som kan ha den här filen öppen”,
  `fileOperations.transferProgress.operationBlockedToast` ”Något annat är öppet här. Stäng det …”) · `high`.
  Pluralvarianten byter bara pronomen: `allt de har öppet där`. Verbet böjs inte efter numerus i svenskan, så
  `{apps} använder` bär två eller fler namn utan någon ändring.
- **`other apps` → `andra appar`, obestämt** · listans sista led, sammanfogat av `Intl.ListFormat('sv')` till ”Preview,
  Warp, Photos och andra appar” (kört 2026-09-16) · `high`. Obestämd form, eftersom ledet står för en öppen rest, inte
  för en känd mängd: `de andra apparna` hade påstått att läsaren vet vilka de är. ❌ Inte `andra program`: `style.md`
  har redan avgjort `app` mot `program` för macOS 26 (mappen heter `Appar`), katalogen säger
  `Stäng öppna filer och appar` om exakt det här läget, och `@key` säger att ordet också ska täcka kommandoradsverktyg,
  vilket `appar` gör i Finders eget språkbruk (”Avsluta alla öppna appar”).
- **disk image → `skivavbild`, bestämt `avbilden`** · macOS Finder `sv` genomgående (`BN53` ”Bränn skivavbilden ”^0” på
  skiva…”, Infofönstrets `tvy-hx-Gou.title` ”Skivavbild:”, lästa i pilen 2026-09-16) · `high`. En-genus, alltså ”En
  skivavbild … är fortfarande öppen”. Andra meningen kortar till `avbilden` och hoppar över ett andra `mata ut`: ”Mata
  ut avbilden först och sedan enheten.” ❌ Inte Microsofts `avbildning`, som är Windows-sidans ord
  (`Windows-avbildning`, `startavbildning`) och aldrig Apples.
- **”stored on this drive” → `som ligger på den här enheten`** · `ligga` är katalogens verb för var en fil finns
  (`fileOperations.cancelRollback.stoppedDeleting` ”Resten ligger kvar.”, § A drive pulled mid-transfer) · `high`.
- **”is still working with” → `arbetar fortfarande med`, inte `använder`** · engelskan skiljer medvetet systemets
  `working with` från appens `using`, och katalogen har redan `arbetar fortfarande` om just indexeringen
  (`fileExplorer.imageIndex.drive.indexing`), vilket är precis det Spotlight gör med enheten · `high`. `macOS` står som
  subjekt utan böjning, som Finders `LA10` (”Objektet ”^0” används av macOS …”).
- **”Wait a minute” → `Vänta en minut`, mot `Wait a moment` → `Vänta en stund`** · engelskan skiljer på hur länge, och
  `Vänta en stund` är katalogens satta form för det korta väntandet (`errors.listing.resourceBusy.suggestion`,
  `errors.listing.deletePending.suggestion`, `errors.write.deletePending.suggestion`) · `high`. Skillnaden överlever
  alltså till svenskan utan att någon av dem blir en ny formulering.
- **”Cmdr itself” → `Cmdr själv`** · katalogens egen form (`indexing.staleDialog.bodyPhone`: ”de ändringar Cmdr själv
  gör”) · `high`. Subjektet först, `fortfarande` kvar på sin vanliga plats: ”Cmdr själv använder fortfarande den här
  enheten.”
- **”send a report” → `skicka en rapport`, och ”if it keeps happening” → `om det fortsätter`** · katalogen säger båda
  ordagrant (`settings.updates.errorReports.description` ”skicka en manuell rapport från Hjälp-menyn”,
  `errors.listing.resourceBusy.suggestion` ”Om det fortsätter, …”) · `high`.
- **Kommat före `eller` står kvar i `unmountRefusedByCmdr`** · `style.md` stryker det mellan två KORTA huvudsatser, men
  här hänger ett `och`-par framför, så kommat markerar vilket led `eller` delar — samma lösning som
  `servers.hub.emptyMessage`.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `Typ`** · macOS Finder `sv`
  `ArrangeByMenu` `119.title`/`338.title`, the Kind sort criterion · `high`.
- **"Select all with extension `*.{extension}`" → `Markera allt med filtillägget *.{extension}`** · Total Commander
  (`WCMD.INC` `527` → `Markera alla filer med samma suffix`) name this exact command, and the mask replaces their "same
  extension" because Cmdr shows the concrete one · `high`. Masken följer på `med filtillägget`, så inget böjs efter
  `{extension}`. TC `sv` säger `suffix`, men katalogens satta term är `filtillägg` (macOS Finder), och den vinner.
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `Markering`** · the catalog's settled noun for
  the SET of selected files, from `commands.selectionSelectFiles.description` (”… i markeringen”) · `high`. Its siblings
  in that menu are verbs; this one names what the submenu holds. ❌ Not the verb `Markera`, which is `menu.bar.select`.
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`)

Varningsbrickan i namnlisten och dess tooltip, som visas så länge Cmdr saknar full skivtillgång; ett klick öppnar
introduktionen på steg 1.

- **`onboarding.fdaBadge.label` → `Ingen full skivtillgång`** · reuses the settled `Full skivtillgång`, re-confirmed on
  a newer OS (`SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable`, key `ALL_FILES`, macOS 27.0
  build 26A428, verified 2026-09-16) · high.
- **`onboarding.stepAi.bannerTitle.denied` already carried this exact string**, and `i18n-terms` holds the two identical
  because they share one English source. ❌ Reword neither alone.
- **`onboarding.fdaBadge.ariaLabel` opens with the label verbatim**
  (`Ingen full skivtillgång. Öppna steget om full skivtillgång i introduktionen.`), which is what satisfies `i18n-aria`
  (WCAG 2.5.3). The definite-form trap applies here: ❌ never `skivtillgången` in the aria sentence, since the definite
  form would stop containing the indefinite label. ❌ Re-wording the label alone breaks it too.
- **Tooltip terms**: drive → `enhet` (settled, as in `onboarding.stepOptional.indexing.benefit1`), search a drive →
  `söka igenom` (settled), cloud folders → `molnmappar` (the compound, like the settled `molnleverantör`), "files macOS
  keeps to itself" → `filer som macOS håller för sig själv` (plain, ❌ never a macOS feature name), "Click to …" →
  `Klicka för att …` (as in `fileExplorer.breadcrumb.navigateTooltip`), onboarding → `introduktionen` (as in
  `shortcuts.scope.onboarding`).

## The trash refusal dialog (`errors.write.trashRefused.*`)

macOS turned down a move to the trash, and Cmdr now words the refusal three ways (permission-shaped, no trash at that
location, unclassified) instead of one sentence. RAW family, so single apostrophes and `{count}` is a literal
replacement target. Four rules bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical. ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. Swedish solves it
  with `{count} av objekten du valde`, which takes any numeral without agreement.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails`
  (`Tekniska detaljer`), because it points at that very control.

- **locked → `låst`** · macOS (`AXNODE1` `Låst`) and the settled catalog term · high.
- **"delete them permanently" → `radera permanent`** · matches `commands.fileDeletePermanently.label`, so the suggestion
  names the command the user will run · high.
- **badge (the title-bar pill) → `märket`** · Microsoft sv terminology (`badge` → `märke`) · high. ❌ Not
  `statussymbol`, which this termbase reserves for the small overlay marker on a file row. title bar → `namnlisten` ·
  high.
- The quoted badge text is `onboarding.fdaBadge.label` verbatim, in the Swedish `”…”` quotes the catalog uses throughout
  (214 of them, zero `“`).
- "somewhere macOS keeps to itself" → `på ett ställe som macOS håller för sig själv`, reusing the wording settled for
  `onboarding.fdaBadge.tooltip`. Plain, ❌ never a macOS feature name.

## Varningen om innehåll som bara finns online (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Om ett markerat objekt i en molnmapp bara finns online skulle papperskorgen hämta hem det först. Därför öppnar Cmdr
dialogen för att radera permanent och förklarar det i varningsrutan. Två varianter av rutan: en för en blandad
markering, en för en markering som helt och hållet bara finns online. De skiljer sig bara i första meningen och i vilka
utvägar de kan erbjuda. Den tredje nyckeln är raden som visas när Cmdr lämnar tillbaka en tryckning.

- **`.cloudOnlineOnlyMixedWarning`** · `finns endast online` är Finders formulering för en utrensad fil; `papperskorgen`
  och `molntjänst` kommer från `terms.json` · medium.
- **`.cloudOnlineOnlyAllWarning`** · samma text, med ”Allt du har markerat” i stället för ”En del av det du har
  markerat”, och utan utvägen att avmarkera: om allt bara finns online skulle ingenting bli kvar markerat · medium.
- **`.cloudOnlineOnlyHandedBack`** · raden ovanför knappen efter en tryckning som Cmdr medvetet inte utförde. Saklig,
  utan ursäkter · medium.
- **Alla fyra fakta måste stå kvar**: (1) papperskorgen skulle hämta filerna, (2) därför erbjuder Cmdr bara att radera
  HELA markeringen, (3) efteråt finns INGEN kopia i papperskorgen, men tjänsten har en egen (❌ tona inte ner det), (4)
  de utvägar rutan nämner.
- **De två `<strong>`-partierna står kvar**, på ”hämta dem först” och på verbet ”radera”. Och ”Radera” inom citattecken
  är knappens etikett: alltid samma ord som `fileOperations.delete.confirmDelete`.
- **`markering`, inte `urval`**: selection är katalogens `markering` (`terms.json` `select`), och
  `.cloudOnlineOnlyHandedBack` säger därför `Det du markerade`.
- Titta på den vid overflow-kontrollen: rutan är lång och sitter i en smal remsa ovanför fillistan.

## När servern svarar att den delade mappen inte finns (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Det enda fallet i familjen där ett nytt försök inte hjälper: servern svarar tydligt att den inte har någon delad mapp
med det namnet. Därför står ingen knapp bredvid den här aviseringen, och tonen får inte antyda något tillfälligt (inget
`just nu`, inget `försök igen`) till skillnad från systrarna.

- **"the server says it has no share by that name" →
  `eftersom servern säger att den inte har någon delad mapp med det namnet`** · katalogen (`errors.mount.shareNotFound`)
  och NetAuthAgent `EINFO_NO_SHARE` (LEVANDE paket, macOS 26.6.2, 25G83, 2026-09-17) · `high`. ⚠️ Apple säger där
  `Delningspunkten ”%@”`; katalogen håller fast vid `delad mapp`, som `style.md` satte.
- **`eftersom`, inte `för`** · macOS `sv` skriver bisatsen så (`eftersom den inte hittades på nätverket`, `PE76`), och
  `för` drar ner i talspråk mitt i en förklarande mening · `high`.
- **"This one won't sort itself out" → `Det här löser sig inte av sig självt`** · ingen källa i högen; det är den
  vanliga svenska vändningen och bär precis det som skiljer aviseringen från de andra: att vänta hjälper inte · `high`.
- **"may have been renamed or removed" → `kan ha döpts om eller tagits bort`** · ordagrant från
  `errors.write.destinationNotFound.suggestion` · `high`.
- **"so it's worth checking there" → `så det är värt att kolla där`** · `kolla` håller den lediga tonen som `style.md`
  vill ha; `där` pekar tillbaka på servern utan att upprepa ordet · `high`.
- **"a lot slower" → `mycket långsammare`** · här ger engelskan ingen multiplikator, till skillnad från systern med
  `fyra gånger` · `high`.
- **Den korta aviseringen säger `säger sig inte ha`** i stället för `säger att den inte har` · annars står två `den`
  bredvid varandra och läsaren måste gissa vilket som är servern och vilket som är mappen; `säga sig` + infinitiv är
  dessutom den vanliga svenska formen för ett påstående som någon annan står för · `high`. Slutet
  `så den ligger kvar på systemanslutningen` är systrarnas (`fileExplorer.pane.directConnectionUnreachableToast` …).

## Namn som ser likadana ut men stavas olika (`errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`, `fileOperations.transferProgress.lookAlikeHint`)

Två namn på en server som ser identiska ut men lagras som olika tecken (ett `é` som ett tecken eller som `e` plus
accent, eller olika versaler/gemener). Återanvänder `objekt`, `server`, `mapp` och `skriv över`.

- **"spells them differently" → `stavar dem olika`; "stores them spelled differently" →
  `lagrar dem med olika stavning`** · vardaglig bild som engelskan, ingen teknisk term (inga `Unicode`/`normalisering`,
  som `@key` kräver). ❌ Inte `lagrar dem olika stavade`: participet hänger inte ihop med `lagrar` · `high`.
- **"upper and lower case" → `stora och små bokstäver`** · macOS Finder sv (`LocalizableMerged`: "den volymen skiljer
  inte på stora och små bokstäver i filnamn"). `skiftlägeskänslig` finns i Dolphin/TC men är för tekniskt för en
  förklarande mening · `high`.
- **"the folder above" (föräldern) → `den överordnade mappen`** · Finder sv "Öppna överordnad mapp", och katalogens
  `errors.listing.notFound.suggestion` ("Gå till den överordnade mappen …"). `mappen ovanför` kan läsas som en rad
  ovanför i listan · `high`.
- **"choose it from its folder" → `välj det i mappen där det ligger`** · `välja` (inte `markera`) eftersom det gäller
  att välja vilket av två objekt som avses; ❌ inte `dess mapp`, som låter stelt · `high`.
- **Knappnamnet som subjekt: `Skriv över ersätter det som redan finns där`** · samma form som
  `fileOperations.transferProgress.rollbackAlreadyLandedTooltip` ("Avbryt stoppar ändå Cmdr …"): knappens etikett
  ordagrant, utan citattecken · `high`.
- **`{path}` i `errors.volume.ambiguousName` står inom `”…”`**, som i systrarna
  `errors.volume.notFound`/`.alreadyExists` · `high`.

## Strömbrytaren ”Tillåt moln-AI” och lägena när moln-AI är avstängt (`ai.cloudConsent.*`, `askCmdr.gate.*`)

Ett samtycke för integritet: av som standard, och ingenting lämnar Macen förrän det slås på. Texten ska vara lugn och
aldrig lova mer än Cmdr gör. Belägg från det installerade macOS (referenssamlingen finns inte på M1), enligt
`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?".

- Allow cloud AI (strömbrytarens etikett, `ai.cloudConsent.label`) · **Tillåt moln-AI** · `Allow` → `Tillåt` är knappen
  i macOS sv:s behörighetsfrågor (`TCC.framework` `Localizable.loctable`, `REQUEST_ACCESS_ALLOW`, macOS 26.6.2 build
  25G83, läst 2026-09-23); `Moln-AI` är det redan publicerade värdet för `settings.ai.provider.opt.cloud`, alternativet
  precis ovanför · `high`
- cloud AI (substantiv, neutrum som `AI`) · **moln-AI**; avstängt läge · **Moln-AI är avstängt**, samma form som
  `ai.translateError.off.title` (`AI är avstängt`); pronomen `det` (`Tillåt det i Inställningar > AI`) · `high`
- **Etiketten återges ordagrant.** Imperativet `Tillåt moln-AI` är samtidigt etiketten, så `askCmdr.gate.cloudOff.body`
  och `settings.askCmdr.cloudOffHint` skriver det utan citattecken, medan `settings.ai.cloudConsent.lockedHint` citerar
  den med `”…”` efter `Slå på`, som engelskan.
- `Ask Cmdr` som subjekt tar `den` (en-genus, som `Cmdr … när den …`): `Ask Cmdr är avstängd`, `Slå på den`. Turn on →
  `Slå på` (katalogens och macOS verb, se § Serverhubben: tabellen, statusarna och tomläget) · `high`
- AI service / cloud AI service · **AI-tjänst** / **moln-AI-tjänst** (redan i `settings.ai.cloudProvider.description`).
  Engelskan skiljer `service` från `provider` i `ai.cloudConsent.askCmdr.*`, som säger `leverantör`; båda står kvar.
- custom endpoints · **anpassade slutpunkter** (`slutpunkt` är redan katalogens ord) · `high`
- side panel · **sidopanel** (Thunar `Sidopanel`, `panel` är katalogens ord) · `high`
- Funktionsnamnen i utfällningen (inom `<b>`): `Namnförslag för nya mappar`, `Sök med vanliga ord`,
  `Markera med en beskrivning` (select → `markera`, macOS `Markera allt`) · `high`
- Settings > AI · **Inställningar > AI**, med `>` som syskonen i `ai.translateError.*`.
- `settings.askCmdr.enabled.label` = `Ask Cmdr`, identiskt med engelskan och med `sameAsSourceJustification`
  (produktnamn, som `settings.section.askCmdr`).

## Escape lämnar helskärmsläge (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- **full screen: `helskärmsläge`**, "exit full screen" → **`lämna helskärmsläge`** · macOS sv AppKit `MenuCommands`
  (`Exit Full Screen` = "Lämna helskärmsläge", `Enter Full Screen` = "Helskärmsläge") och Finder `FV20`/`FV21`, samma
  par. Bestämd form när det är tillståndet man lämnar: "tog Cmdr ur helskärmsläget". `high`.
- **Escape (tangenten): `Escape`**, naket, som katalogens `Retur` · macOS sv AppKit `FunctionKeyNames` behåller
  "Escape"; AppKits löptext säger "escape-tangenten". `shortcuts.section.pressEscToClear` skriver `ESC` (tangentkappan);
  i hela meningar använder vi Apples `Escape`. `high`.
- Etiketten `Lämna helskärmsläge med Escape` delas ordagrant av `main.escapeFullScreenHint.switchLabel` och
  `settings.advanced.exitFullScreenOnEscape.label`: skriv om båda eller ingen.
- Settings > Advanced · **Inställningar > Avancerat**, med `>` som `fileExplorer.quickLookHint.configurable`.

## Original som ändrades under flytten (`transfer.changedDuringMove`, 2026-09-25)

- **”changed during the move” → `ändrades under flytten`** · Finder `PE56` ”ett eller flera objekt ändrades under
  bränningen” (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25) · `high`. `under flytten`,
  `blir kvar` och `källmappar` tas ordagrant från syskonet `transfer.appearedDuringMove`, som står i samma meddelande.

## Väntraderna i ”Öppna med” och ”Dela” (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## Väntraderna i ”Öppna med” och ”Dela” (`menu.context.openWithLoading`, `menu.context.shareLoading`/`.shareNone`)

- **Finding apps…: `Söker efter appar…`** · presens som macOS (”Searching…” = ”Söker…”), `appar` som
  `settings.behavior.textEditorApp.checking`. `high`.
- **share options: `delningsalternativ`** · `delning` från undermenyn `Dela` + termbasens `options: Alternativ`; inte
  `delad mapp`, som är nätverksbetydelsen. `tentative` (sammansatt, inget belägg i högen).
- **No share options: `Inga delningsalternativ`** · macOS mönster för tom meny (”No Services Apply” = ”Inga tjänster
  tillgängliga”). `high`.

- Spotlight, Mission Control, Spaces stay English. `Teckenvisare`, `Avsluta tvingat`, `byte av inmatningskälla` are
  Apple-style and unverified (review queue). `modifierare` because MS's `låstangent` is the lock-key sense.

## Ask Cmdr chat (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- `svar` names "this one" / "the reply" in `askCmdr.error.budgetExhausted` / `.unfinishedReply` (`Svaret …`): a bare
  pronoun has no gender-neutral Swedish antecedent.
- `token` / `tokens` stays English in both branches (Swedish tech press; the pile's `säkerhetstoken` is another sense).
- Cmdr's own behavior repeats `Cmdr` across sentences over `den`/`det` (`askCmdr.empty.hint`,
  `ai.cloudConsent.askCmdr.contentsRule`); a pronoun is fine when its antecedent sits in the same sentence.

## Ask Cmdr names only the chat panel (`askCmdr.wake.needsFullDiskAccess`, `settings.ai.tooltipOff`, `settings.ai.provider.description`, `settings.askCmdr.proactive.description`, `ai.cloudConsent.askCmdr.proactive`)

- `Ask Cmdr` stays only where the key's current English keeps it (the panel, its menu item, command, and settings);
  elsewhere the subject is `Cmdr` or `AI:n`, as that key's English says. Read the current English, not memory.
- `chatt` and `samtal` aren't interchangeable: each follows its own key's `chat` / `conversation`.
- `needsFullDiskAccess`'s second sentence copies `search.coverage.setUpFullDiskAccess`, which `@key` requires.

## Allow cloud AI (`ai.cloudConsent.*`, `askCmdr.gate.*`)

- `Tillåt moln-AI` (TCC's `Tillåt`, the shipped `Moln-AI` option). `moln-AI` is neuter: `Moln-AI är avstängt`, pronoun
  `det`. `Ask Cmdr` as a subject takes `den` (`Ask Cmdr är avstängd`).
- The imperative doubles as the label, so it's written unquoted where it's an instruction and quoted with `”…”` in
  `settings.ai.cloudConsent.lockedHint` after `Slå på`, as the English.
- `service` → `tjänst` and `provider` → `leverantör` both stay: the English distinguishes them.

## Ask Cmdr looks inside files (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- A photo's location → `var den togs`, never `plats`: right after `en bilds`, `plats` reads as where the FILE lives.
- `kamerauppgifter` over `kameradetaljer` (`detaljer` is the expandable technical-details section). `miniatyr` over
  Nautilus's `miniatyrbilder` (first-party word).
- "the list of files inside an archive" → `vilka filer som finns i ett arkiv` (a list-of calque reads badly).
- A list whose last item contains `och` takes `samt` before it (`… samt en bilds kamerauppgifter och var den togs`).
- `askCmdr.empty.hint` and `settings.askCmdr.intro` no longer promise `skrivskyddad` / "never changes anything": Cmdr
  writes notes and proposes renames, so they say it never changes a file without approval.

## Image indexing: network drives, scope, badges, progress (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.networkOff`/`.paused`, `askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`, `askCmdr.tool.searchPhotos.*`, `askCmdr.tool.imageFacts.*`)

- photo → `bild` uniformly (Apple's Photos app is `Bilder`), so the feature reads as one word; the one legitimate
  `foton` is `onboarding.stepOptional.mtp.desc` (copying photos off a phone, not the feature).
- exclude → `utesluta`, never `undanta` (the pile's `undantag` only means exception). A passive per-image state is
  `Ingår inte i bildsökningen`, distinct from the user action.
- rename noun `namnbyte` (Thunar / Dolphin; macOS has only the verb), never `filbyte` (reads as swapping files). The
  warning badge is the noun `(överskrivning!)`: an imperative badge would command the overwrite it blocks.
- needs attention → `behöver ses över` over the calque `kräver uppmärksamhet`.
- caches → `cachemappar`: Swedish has no settled plural of `cache`, and the sentence means cache folders.
- "Ask Cmdr to prepare it again" → `Be Cmdr att …`: the English "Ask" is the verb, not the feature.
- Badges agree with the en-word `bild` (`Indexerad`), queued `Väntar på att indexeras` (Finder's badge pattern), active
  `Indexeras nu`, re-index `indexeras om`. Drive plurals put the tail inside both branches so `indexerad(e)` agrees.
- search by description → `sökning med beskrivning` (bare feature noun), toggle `Sök bilder med en beskrivning`.
- `Apple silicon` stays English and lowercase (the pile lacks Apple's `Apple-kisel`; the key says keep it).
- A network drive opts in with `välja in` internally and `aktivera` on the toggle; gently → `skonsamt`.

## Delete dialog trash switch, transfer From/To (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`.targetGroupTitle`)

- The switch reads `Flytta till papperskorgen`, identical to `transferDialog.titleVerbOnly`'s arm, so switch and button
  read as one pair. Headings `Från` / `Till` (Total Commander); the controls keep `mål` (`Målvolym`).

## Drive indexing's master switch (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`/`.overriddenBadge`, `settings.indexing.enabled.label`, `settings.section.driveIndexing`, `settings.summary.driveIndexing`)

- `<X> indexing` is a compound (`enhetsindexering`), never `indexering av <X>`: Dolphin's `Filindexering`, zero
  `indexering av` in the pile, and a bare singular after `av` is ungrammatical anyway.
- A switched-off feature `är avstängd`, never `är av`.
- The overridden badge is `Kräver enhetsindexering`, never `Av med …`: `av med` reads as "off with it!", an imperative
  badge. `Kräver X` is the catalog's cause pattern (review queue).
- stays unindexed → `indexeras inte` over the unattested `oindexerad`.

## Drive index change check (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- The run header is nominal, `Kontroll av ändringar`, to match its sibling headers; `kontroll` over colloquial `koll`
  (which lives only in the idiom `tappade koll på`). The running check is `genomsökningen`.

## Stalled transfer (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

- No source has "stalled": `Inget har hänt på {duration}` (fits the queue row where it replaces `{duration} kvar`) and
  `står stilla` over `har stannat`, which overclaims that the transfer is over. `förlopp` is the progress indicator,
  `framsteg` the achievement sense; neither fits.
- "Waiting for X to respond" → `Väntar på att X ska svara` (Finder verbatim).
- `Stäng` for the button that leaves the transfer running; `Detaljerna finns i loggfilen`, since `åtgärdsloggen` is the
  other log.
- `stallInFlight` puts the tail inside both branches: `öppen … skriven` vs `öppna … skrivna` agree with the count.

## Copied path (`fileExplorer.clipboard.copiedPath`)

- The path shows on its own line, so the sentence ends on a colon and stands alone: `…, den finns nu i urklipp:`.
  `i urklipp` (settled), no possessive: there's only one clipboard.

## Operation queue (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

- English widened "Transfer queue" to the category word, so Swedish does too: `Åtgärdskö`, paired with `Åtgärdslogg`.
  `överföring` stays right one level down (a copy or move in flight). ❌ Never `överföringskö` / `Överföringar`.
- The window title and the command label are indefinite `Åtgärdskö`; prose, including prepositional button labels
  (`Visa i åtgärdskön`), is definite. Don't flatten the two.

## Progress chip and failure notice (`queue.row.dismiss*`, `queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`)

- `Avfärda` over MS's `stäng` (AppKit `Avfärda popover`, and `Stäng` is reserved for windows). ❌ Not `Ta bort`: on an
  operation row it reads as re-deleting the files.
- Headline family `Gick inte att slutföra` + definite verbal noun (Finder's
  `Det gick inte att slutföra synkroniseringen`), `skapandet av mappen` over a compound because English points at THE
  folder. The headline stays clipped, byte-identical to the row's failed arm; body copy keeps `Det gick inte att …`.
- The aria spells `procent`; the visible tooltip keeps `{percentText} %`.
- `{label}` plus the count clause wobbles ("Flyttar till papperskorgen 3 objekt"), as English does; the fix belongs to
  the English key's shape, not one locale.

## Standalone conflict prompt (`fileOperations.operationConflict.context`/`.pausedNote`)

- The context arms are `queue.row.label`'s verbs plus `till {destination}` (Finder `Kopierar ”^1” till ”^2”`),
  destination unquoted as in the English. The fallback takes `i` (`Arbetar i …`): where work happens, not where items
  go. `Redigerar ett arkiv` needs the article because it's a sentence.
- `Allt annat är pausat` (neuter `Pausad` after `allt annat`) so the note and the rows read as one state.

## Queue button with an empty queue (`fileOperations.transferProgress.background`/`.backgroundAria`)

- `I bakgrunden` (Total Commander's neighbouring button ID in the same dialog). ❌ Not bare `Bakgrund` (the backdrop, a
  label not a command, and not contained in the definite `i bakgrunden`). `Kör i bakgrunden` is the fuller reserve.
- The aria `Håll igång den här i bakgrunden` is byte-identical to `queueTooltip`'s opening; containment is
  case-insensitive (`I` / `i`).

## Quit gate (`main.quit.*`)

- `Avsluta medan en åtgärd pågår?` (Finder's `en åtgärd fortfarande pågår`), keeping Cmdr's `åtgärd` over Total
  Commander's `aktivitet`.
- The countdown says `Cmdr avslutas om …`: an app quitting itself takes the deponent `-s`, and active `Avslutar`
  collides with Finder's progress stage "Finishing".
- `rensar bort` over `raderar`, which is the user's own destructive delete.
- `Fortsätt arbeta` / `Avsluta nu`; ❌ not `Avsluta ändå` ("should I at all?"), since this button skips the wait.

## Usage stats without "anonymous" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

- The stats carry a stable random id, so ❌ never `anonym`, and ❌ never `pseudonym(iserad)`: that jargon is what the
  English avoids. `ett slumpmässigt id` over clunky `identifierare`; `kopplad till`.

## Awaiting-answer row and rollback confirm (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- `Behöver ditt svar`, ❌ not `Väntar på svar`: the same column shows `Väntar` for queued rows.
- The rollback tooltip's stop is `Stoppa`, ❌ never `Avbryt`: that IS the Cancel button, which keeps the files.
- Overwritten files are `filer som den har skrivit över`; English "replaced" is the overwrite sense, not `ersätta`.

## Rollback family: `ångra`, not `återställ` (`fileOperations.transferProgress.*`, `operationLog.*`, `commands.logOperationLog.*`, `fileOperations.cancelRollback.*`)

- `återställa` is restore, and a rollback deletes what it wrote; MS's `återställa` for roll back is the database sense.
  The catalog keeps `återställa` for names that really come back (`askCmdr.renameUndo.*`) and resets.
- `Ångra klart` (Finder's `kopiera klart`) finishes a half rollback; ❌ not `Slutför ångringen` (the noun `ångring` is
  stilted, and the button sits in a list row). `operationLog.dialog.finishRollBack` and
  `fileOperations.rollbackConfirm.finishRollBack` stay identical; `Ångra klart den här åtgärden?` mirrors the title.
- `partiallyRolledBackNotice` says `lät resten ligga kvar`, ❌ not `som den var` (reads as "as before the operation").
- `smbNativeNote` uses the verb (`Det kan ta tid att avbryta eller ångra`) over the noun `ångring`.
- The tooltip `rollbackTooltipStopAndMoveBack` uses `lägg tillbaka`; ❌ never `radera`: a move's rollback deletes
  nothing.

## Rollback toast (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- Reason lines copy `askCmdr.renameUndo.skipReason.*` word for word where the English matches (`… lämnades som den är`);
  `spotTaken` switches frame with the English (`lämnades där den ligger`), and says
  `något annat finns nu där den kom ifrån` so it can't sound like the neighbouring name-taken reason.
- Removed → `Raderade` (files off disk), ❌ never `Tog bort`.
- "all {countText} items" → `alla {countText} objekt` (a quantifier, no article), and `one` drops the number
  (`Raderade objektet som Cmdr hade skrivit`). `stoppedDeleting` says
  `där Cmdr lade dem` (it covers copy and compress), `stoppedMovingBack` `där flytten lade dem`.
- `stagedLeftover.*` is Cmdr's own work file: `ofullständig kopia`, `rensar bort`, and `vid en senare överföring`, ❌
  never "nästa gång": cleanup skips anything younger than an hour, so the next try may clear nothing.

## Rename toasts: chained and unconfirmed (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, `fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

- The two families mean opposite things (definitely kept vs unknown) and must never blur.
- "kept its name" → present `behåller sitt namn` (Total Commander's `Behåll namnet`): the state the file is in.
- "and so did …" → `, liksom …`. ❌ Not `och det gör …` (unreadable without a comma) nor bureaucratic
  `och detsamma gäller`.
- Unconfirmed follows `fileOperations.mkdir.timeoutMessage`: `så filen kan ändå ha bytt namn`. ❌ Not `gått igenom` or
  `lyckats` (the house voice avoids that status word). Several renames take the definite plural `namnbytena av`.
- `Det här mappnamnet / filnamnet kan inte användas` (Finder's `Namnet … kan inte användas`), no final period.

## Suggested operations (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- approve → `Godkänn` over macOS's `Ta emot` (AirDrop receiving); reject → `Avböj`.

## Duplicate (`commands.fileDuplicate.*`)

- `Duplicera` (Finder's File menu); no collision with `Kopiera` (F5).

## Native menus (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Raw family: single apostrophes, a doubled `''` shows twice on a real menu.
- The View menu is `Innehåll`, not `Visa`: Finder AND Safari agree, so it's Apple's standard, not a Finder quirk.
- `Window > Zoom` is the verb `Zooma`, the text-zoom submenu the noun `Zoom`; English says `Zoom` both times.
- Pin / unpin tab → `Fäst flik` / `Lossa flik` over Safari's `Nåla fast`: `lossa` is the natural opposite and the
  catalog already uses the stem. Changelog → `Ändringslogg`, apart from Help's `Nyheter`.
- `Öppna i redigeraren` over the stem-repeating `Redigera i redigeraren`.
- The tag row reads `Lägg till ”{color}”` / `Ta bort ”{color}”` (Finder). The selection submenu title is the noun
  `Markering`, not the verb `Markera` (`menu.context.selection`).
- The Dock menu (`menu.dock.*`) takes the app-name form `Öppna Cmdr`, ❌ not the file-name form `Öppna ”Cmdr”`, which
  reads as opening a file. `{name} ({parent})` stays as punctuation.
- `menu.network.open` is `Öppna` like `menu.file.open` (same sense); `menu.network.edit` copies
  `commands.serversEdit.label` byte for byte.
- `menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` and their `commands.selectionSelectSameKind.*`
  twins share one English each: reword neither alone. `*.{extension}` follows `med filtillägget`, so nothing inflects.
- Loading rows: `Söker efter appar…`, `delningsalternativ` (from `Dela`, not the network `delad mapp`).
- Justified same-as-source: `menu.view.zoom`, `menu.tag.orange`, `menu.view.askCmdr`.

## System SMB connection fallback (`fileExplorer.network.osMountFallback.*`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- native → `inbyggd`, ❌ not MS's `ursprunglig` (original format). `inbyggd` describes; `systemanslutning` names the
  same path elsewhere. The English "network" is dropped: `SMB-nätverksanslutning` is a heavy triple compound.
- "share not on server" is the one case where retrying won't help: no `just nu`, no `försök igen`. The short toast says
  `säger sig inte ha` so two `den` don't sit side by side.

## Mutation and volume errors (`errors.mutation.*`, `errors.volume.*`)

- Raw family. A volume's top folder is `rotmapp` (Thunar, Total Commander; macOS has none). System Integrity Protection
  stays English (Finder `ET6`).
- No blame on the person: `Ett namnbyte kan inte flytta ut ett objekt ur ett arkiv`, not `Du kan inte …`.
- `svarade inte i tid` over `tidsgränsen nåddes`: shorter, and names who went quiet. The change "may still land" →
  `kan fortfarande gå igenom`.
- `notSupported` / `ioError` need a head noun for the English "that": `den åtgärden`.
- "has no Trash" is indefinite `har ingen papperskorg` (no such thing); `macOS nekade flytten till papperskorgen` names
  macOS as the refuser, as the English does, ❌ not the impersonal `Det gick inte att …`.

## Crash dialog variants and the reports setting (`crashReporter.dialog.body.keptRunning`/`.unknown`, `settings.updates.crashReports.description`)

- `.keptRunning` and `.unknown` never say Cmdr crashed, quit, or stopped, and say `en rapport`, not `kraschrapport`.
- `stötte på ett problem` keeps `Cmdr` as subject like `.ended`; every pile phrasing is impersonal. ❌ Not
  `råkade ut för` (accident tone). kept running → `fortsatte köra` (AppKit's exception dialog); ❌ not `höll igång` (the
  queue's transitive verb) nor `fortsatte fungera` (a feature working, not a process living on).
- The three variants share `förra gången` although Apple says `När du senast …`: siblings must share a frame. If
  `.ended` is ever reworded, `När Cmdr senast kördes …` is the attested alternative for all three.
- The setting's description covers both outcomes in present tense; its label stays `Skicka kraschrapporter`.

## Eject and disconnect errors (`errors.eject.*`, `fileExplorer.navigation.ejectBusyTooltip`, `fileExplorer.navigation.disconnectBusyTooltip`)

- Raw family, and every value follows a colon in `fileExplorer.pane.ejectFailedToast` / `.disconnectFailedToast`, so it
  never repeats the frame's `det gick inte att`; it says why and what to do.
- in use → `används` (Finder), ❌ not MS's `upptagen`, which is reserved for the menu marker `(upptagen)`. idle →
  `när den inte används`, same thread.
- removable → `borttagbar` (Finder), over the MS / Thunar / Total Commander `flyttbar` Apple never uses.
- `koppla från` is programmatic, `koppla ur` the device out of the port, `dra ur` the cable.
- The noun `utmatningen` in `timedOut` (tentative): ❌ not `så den kan fortfarande matas ut`, which reads as "you can
  still eject it".
- Named-app refusals (`unmountRefusedBy*`) keep `unmountRefused`'s frame with `{app}` as a bare subject; active over
  AppKit's passive `används av ”%@”`, and the key forbids quoting the name. `other apps` → indefinite `andra appar`.
  macOS `arbetar fortfarande med` keeps the English split from an app's `använder`; `Vänta en minut` vs `Vänta en stund`
  keeps the English split too.
- disk image → `skivavbild`, short `avbilden`; ❌ not MS's `avbildning` (Windows side).
- A disabled button's tooltip says `Det går inte att koppla från medan åtgärder pågår på den här servern`, without the
  menus' `(upptagen)` marker.

## Trash toast (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- put back → `lägga tillbaka` (Finder's `Lägg tillbaka`), ❌ not `återställa` (restore, and used for names) nor
  `flytta tillbaka`: the menu item's word is the action's name.
- `Gå till papperskorgen` is 21 characters against 11; overflow-check it beside `Ångra` (review queue).
- `{skipped}` is a real selector, but `objekt` and the verb don't inflect, so both branches are identical.

## Amending a sent error report (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- add to → `lägga till i` (`Lägg till i Dock`), ❌ not `bifoga`, already taken by the email in the same dialog.
- The menu pointer is `från Hjälp-menyn` (hyphenated proper-name compound), never `menyn Hjälp` or `Hjälpmenyn`.
- `Visa rapporten eller lägg till en notering` keeps both halves and the word `notering` that ties it to the field.

## Selection dialog (`selection.*`)

- `markera` / `avmarkera` for files (Finder `Markera allt` / `Avmarkera allt`); `Välj` is for picking an option.
- `selection.recent.applyAria` mirrors `search.recent.runAria`; Enter is `Retur` everywhere.

## One thing, one name: the term-drift audit (`menu.file.delete`, `commands.fileDelete.label`, `settings.mediaIndex.clip.*`, `commands.selectionSelectAll.label`, `commands.selectionDeselectAll.label`, `fileExplorer.errorPane.goHome`, `menu.select.all`, `menu.select.deselectAll`)

- F8 is `Radera` in the menu, palette, key bar, and dialog: the pair must differ in strength (`Radera` /
  `Radera permanent`), which `Ta bort` / `Radera permanent` doesn't. The AI model is `Radera`d like `ai.local.*`;
  `settings.mediaIndex.reclaim.*` keeps `ta bort` (index rows, not files).
- A bare English `All` is `allt` (`Markera allt`, `Återställ allt till förval`): `alla` dangles without a head noun.
  With a head noun it inflects normally (`Stäng övriga flikar`, Safari's `övriga` over `andra`).
- `Kopierat` (supine) for a bare "Copied": it fits whatever was copied, and `ett id` is neuter anyway.
- `Gå till hemmappen` joins the `Gå till …` family. `mapp` / `mappar` over the abbreviation `kat.`.

## Same concept, different English (`commands.viewShowHidden.label`, `settings.fileViewer.suppressBinaryWarning.label`, `settings.fileExplorer.suppressQuickLookHint.label`)

- Hide → `Göm` (eleven Finder strings, zero `Dölj`); hidden → `dold` (the file-system term). Suppress is another verb
  and keeps `Dölj`. So `Visa eller göm dolda filer` avoids `dölj dolda`.
- `mappstorlekar` over `katalogstorlekar`; `katalog` stays where English means a technical directory.
- share → `delad mapp`; `-resurs` only inside a compound (Swedish can't compound a two-word phrase); `delning` only in
  the share counter.

## Boundaries: both forms are right, don't flatten them (`updates.status.checking`, `ai.local.statusRunning`, `operationLog.status.running`, `shortcuts.section.filterModified`, `menu.bar.file`, `menu.bar.view`, `menu.bar.select`, `menu.view.zoom`, `menu.window.zoom`)

- Checking: `Checking for X` → `Söker efter X` (looking for something that may exist), `Checking X` → `Kontrollerar X`
  (verifying something you have).
- Running: a process `Körs`, an operation `Pågår` (Finder's minimal pair).
- `unknown` agrees with the implied head noun: every shipped one is an en-word (`okänd`).
- Modified: `Ändrad` on one file's attribute, `Ändrade` on the filter chip over a set.
- File, View, Select, Zoom: menu-bar title vs action, as in § Native menus.

## Menu and palette twins, System Settings panes (`menu.app.hideOthers`, `commands.appHideOthers.label`, `menu.app.showAll`, `askCmdr.renameUndo.undone`/`.partial`)

- `Göm övriga` in both the menu and the palette (three Apple bundles), ❌ not `Göm andra`.
- The git and provider errors use the runtime tokens `{system_settings}` / `{privacy_and_security}` /
  `{files_and_folders}`; never hand-translate them or hang an ending on one (`i {system_settings}`).
- Restored names read `Det gamla namnet återställdes … fil` / `De gamla namnen återställdes … filer`, both branches
  spelled out because noun and participle agree; trash stays `Lade tillbaka`.

## Old WebKit and old macOS screens (`main.oldWebkit.*`, `main.oldMacos.*`)

- `Programuppdatering` over the anglicized `Mjukvaruuppdatering`. best effort → `så gott det går`; look off →
  `se konstiga ut`, which avoids the forbidden status words. The last sentence is David in first person.

## Open terminal here (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- `Öppna terminal här` builds on Finder's `Öppna i Terminal`; the card title is a justified same-as-source. `Välj app…`
  is Finder's `Choose Application…` verbatim.

## Sort by relevance (`fileExplorer.columns.sortByRelevance`)

- `Sortera efter relevans`, indefinite like the sibling sort labels.

## Server hub: connection state, refusals, forget dialogs (`servers.paneState.*`, `servers.refusal.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`)

- trust → `lita på`, trusted `betrodd` / `betrott` / `betrodda` (SecurityInterface).
- The host key is bare `nyckel` where the English says "key", and `nyckeln från {host}`, never a genitive on the
  uncontrolled `{host}` (it may end in an s-sound).
- Cmdr's reconnect loop `arbetar på att …` (the catalog never says `jobbar`).
- Forget dialogs inherit the menu labels verbatim; `slutar visa servern i listan` names the noun because both
  `anslutningen` and `servern` are en-words and a lone `den` would point two ways.
- `Den här servern använder en nyckel …` mirrors `authMethodUnsupported`; ❌ not `loggar in med`, which makes the server
  the one logging in somewhere.
- Retry lengths (`retryTotalSeconds` / `.retryTotalMinutes`) are bare building blocks for `retryKeepsTrying`: no
  preposition, no period.

## Server hub: table and statuses (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.serverPinnedToast`/`.serverUnpinnedToast`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

- Status cells are en-word participles agreeing with `servern` (`Ansluten`, `Sparad`, `Hittad i närheten`, `Utloggad`);
  if the row noun ever changes gender, rewrite the whole column.
- `Platser` (under a server) is broader than `delade mappar`: buckets will live there too. nearby → `i närheten`, ❌ not
  `upptäckt` (mDNS finds in the browser); English chose the plain "found".
- `Fäst / lossa server` keeps the slash: ONE command toggling both ways, unlike keys whose English says "or".
- `Servern är fortfarande sparad` names the noun: `volymväljaren` and `servern` are both en-words.

## Server hub: connect sheet, host key, root and start folder (`servers.sheet.*`, `servers.hostKey.*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`, `servers.paneState.reconnecting`/`.signedOutNothingToAsk`)

- passphrase → `lösenfras`, ❌ not `lösenordsfras` (an OID field name). `Nyckelns lösenfras` / `Nyckelns fingeravtryck`
  read as a pair; a compound is opaque.
- Swedish can't leave `lita på` bare, so the buttons name the object (`Lita på nyckeln och anslut`), and "I've checked
  it" → `Jag har kontrollerat nyckeln`: both `nyckeln` and `fingeravtrycket` are in the box.
- Remote folder → `Mapp på servern`, ❌ not an unattested `fjärrmapp`.
- try → `prova` (test something); `Försök igen` is reserved for Try again.
- `Lämna fältet tomt`, ❌ not `Lämna den tom`: after `Mappen …` it would mean an empty folder.
- `ditt konto får läsa den` (permission) over `kan`. The two `*NotFound` siblings share a frame.
- `identityLocked` says `identifierar`, ❌ not a "name" verb (the sheet has its own `Namn` field), and uses the button
  verbs `glöm` / `lägg till` verbatim.
- `Öppna servern igen för att försöka på nytt` names the noun after a sentence ending on `nyckel` / `lösenord`.

## Pinned servers, trusted host keys, Android settings rows (`menu.network.*`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.fileOperations.adb*`)

- `Fäst i volymväljaren` / bare `Lossa` keep the English asymmetry; ❌ not `Ta bort` (the server stays listed).
- UI names are quoted by apposition (`listan Servrar`, `Gruppen Nätverk`), since a compound would respell them.
- Got it → `Uppfattat` over macOS's `OK`, which the catalog keeps for a dialog's default button.
- `värdnyckel` only where the English writes "host key".
- Found at {path} → `Hittades: {path}`, ❌ not `i {path}`: the path is the binary itself, not its folder.
- Re-check → `Leta igen`, tying the button to the placeholder `Leta efter adb på vanligt sätt`.
- The picker's title is `Välj kommandot adb` (Apple's picker button `Välj`); the button opening it is `Bläddra…`.
- Location of adb → `Sökväg till adb`, ❌ not Finder's `Plats` (the enclosing folder).
- `platform tools` stays English (the SDK Manager shows `Platform-Tools` in every locale; tentative).

## Android over ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`, `fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

- `USB-felsökning` and `Tillåt` are AOSP's own Swedish, quoted as the names on the phone's screen: ❌ never `Godkänn` or
  `Acceptera`. A phone takes `tryck på`; the Mac's mouse takes `klicka`.
- reseat the cable → `dra ur och sätt i kabeln igen`: a bare `sätt i` doesn't say it comes out first.
- `Koppla från {name}` for a phone matches the server's `disconnectPlaceAriaLabel`: English chose Disconnect over Eject
  on both, and `mata ut` is reserved for ejecting.
- `Du stoppade öppnandet av din telefon`: `stoppa` like `search.coverage.walk.cancelled`, ❌ not `avbröt`, which reads
  as a reference to the Cancel button.
- How → `Hur?` (tentative): Apple has no one-word "How" link, and bare `Hur` reads truncated.
- Turn on a feature in a hint → `Slå på USB-felsökning`; the settings row states `har USB-felsökning aktiverad`.
- The phone's stale index keeps its siblings' frame with `telefonens`; passive `görs` for changes made on the phone,
  since no subject fits (the user or another app).

## Dock offer (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- `Dock` takes no article, inflection, or possessive (`i Dock`), and pin / unpin on Apple's surface are `Behåll i Dock`
  / `Ta bort från Dock` (Dock's own menu), ❌ not Cmdr's `fäst` / `lossa`.
- The Applications folder is `Appar` since macOS 26: `dra … från mappen Appar` (AppKit's model sentence).
- managed by → `styrs av` (Apple's MDM formula); `hanterar` stays for the person administering the Mac.
- `Cmdrs symbol är på plats, men Dock startade inte om`: never claim the pin failed; the icon IS there.
- `Nej tack`, ❌ not `Inte nu`, which promises a later ask Cmdr never makes.
- Never a number for a movable threshold (`några dagar`, `ett tag nu`).

## Show in Finder offer (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- `”Visa i Finder”` quoted as in the settings card; the first-hit notice explains, ❌ never `tyvärr`.
- `settings.revealHandler.notProductionBuild` says `släppt version` / `utvecklings- och testversioner`: a release, not a
  second running `kopia` as in `main.instanceLock.alertBody`.

## AI provider setup (`onboarding.cloudSetup.*`)

- download → `hämta`: `step.install` was the catalog's only `Ladda ner` against 43 `hämta`. Check how often a form
  already appears before writing a new one. deployment → `distribution`, endpoint → `slutpunkt` (MS).

## Onboarding rewrite

### Beta-step checklist (`onboarding.stepBeta.checklist.*`)

- The heading `Checklista för att komma igång` follows the wizard's `komma igång` framing, not `introduktion`.
- A bare English `each` needs a head noun: `varje punkt tar 30 sekunder`.
- GitHub (UI localization ended 2016) and AlternativeTo have no Swedish UI to quote, so their verbs translate:
  `stjärnmärk`, `Gilla` (MS). `repo` is neuter (`repot`).
- The empty `<field></field>` renders the input inside the sentence, so it goes where Swedish wants the object
  (`Ange din e-postadress <field></field>, så …`). `<alpha></alpha>` and `<chip></chip>` get the same reflex.

### Mailing-list signup (`onboarding.stepBeta.signup.*`)

- `e-postlista`, ❌ not MS's `distributionslista` (an Exchange address group). typo → `stavfel`; ❌ Total Commander's
  `Skrivfel!` is a false friend (Write error).
- `signup.rejected` / `.unreachable` quote `checklist.emailSave` (`Spara`) verbatim: a cited label is a unit.

### AI step (`onboarding.stepAi.*`)

- `dummare` stays blunt, as `@key` asks. The `<strong>` block in `local.tooltip` quotes `cloud.label` verbatim
  (`Ja, jag vill ha AI`): change both or neither.
- `<em>och</em>` carries the emphasis alone; ❌ adding `både` makes it redundant.
- `strunta i` over bureaucratic `bortse från`; `badge` → `märke` (MS), ❌ not `aktivitetsikon` (the app-icon counter).

### Optional-step summaries (`onboarding.stepOptional.*`)

- The summaries sit beside switches and must not wrap, so they stay near English length and borrow each `…desc`.
- `macOS inbyggda hanterare`; suppresses a process → `håller tillbaka`, apart from visual Suppress → `Dölj`.

## Viewer fetches first (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- fetch → `hämta`; the heading follows Finder's `Förbereder kopiering av ”^1”` (present verb, quoted name), and
  `för förhandsvisning` needs no pronoun to agree with the file.
- "This file stopped arriving" → `Hämtningen av den här filen står stilla`, the catalog's stall wording.

## Mount and share-list errors (`errors.mount.*`, `errors.shareList.*`)

- share → `delad mapp`, although NetAuthAgent says `Delningspunkten`. this computer → `den här datorn`: the strings also
  show on Linux. `igen … igen` is avoided with `försök på nytt när servern är online igen`.
- Identical English, identical Swedish: the `hostUnreachable` and `authFailed` pairs.

## Text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- `textredigerare` over MS's `textredigeringsprogram`: the catalog's shared stem `redigerare` decides (tentative).
- Shared strings (Choose an app…, Checking your apps…, the hint) copy the terminal picker's.

## Drive disconnected mid-work (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.sided.*`, `errors.write.moveNotConfirmed.*`, `fileOperations.leftovers.stagingFolderKept`)

- In progress `håller på att kopplas från`; already gone `kopplades från`.
- Every sided message ends on WHERE the files are. Reassurance in present, place in past:
  `Dina original är orörda där de låg`; `så ingenting har gått förlorat` (perfect: the state stands).
- `dit` and a bare preposition before `{counterpart}` / `{volumeName}` avoid guessing the name's gender.
- Unconfirmed moves use the house formula `Det gick inte att bekräfta` / `Cmdr kunde inte bekräfta`, `hade sparats på`
  over `skrevs till` (writing is the in-flight transfer), and present `så dina original ligger kvar där de låg` answers
  where the files are now. `Titta i målmappen`, since `Titta i målet` doesn't read.
- The staging folder: unfinished move → `avbruten` (a process ended early) vs `stagedLeftover`'s `ofullständig` (a
  half-made object). `lät dem ligga kvar` says Cmdr CHOSE to keep them, ❌ not `blev kvar`. ❌ Never suggest deleting
  the folder: they may be the only copies.

## Favorites menu (`commands.favorites*`, `fileExplorer.navigation.favorites*`, `fileExplorer.navigation.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- The scope heading is indefinite `Favoritmeny` like its neighbours; prose is definite `favoritmenyn`. No aria quotes
  the heading, so the forms may differ; if one ever does, the heading changes form.
- English See and Show are one action, so both are `Visa`; `Se` is for looking at something.
- `fileExplorer.navigation.favoritesAddCurrent` is the one key for the `0` row, and the shortcut list quotes it: ❌ no
  second, definite wording.
- `a disk` → `disk` (English disk), `enhet` is drive, `skiva` only in Finder's own wording.
- The number keys are `siffra`, not `nummer` (a running number, as in `radnumren`).
- `favoritesCantAddHere` states the reason after a colon with `fungerar bara på`, a flat locative that avoids the split
  `på en disk` / `i en delad mapp`.

## Title-bar full disk access badge and the trash refusal dialog (`onboarding.fdaBadge.*`, `errors.write.trashRefused.*`)

- `onboarding.fdaBadge.label` and `onboarding.stepAi.bannerTitle.denied` share one English: reword neither alone.
- The aria opens with the label verbatim; ❌ never the definite `skivtillgången`, which stops containing it.
- badge → `märket` for the title-bar pill, ❌ not `statussymbol` (the overlay marker on a file row). "somewhere macOS
  keeps to itself" stays plain, ❌ never a macOS feature name.
- The five trash titles (`errors.write.fallback.title.trash` and its `ioError` / `readError` / `writeError` siblings)
  share one English: reword all or none. With no plural machinery, `{count} av objekten du valde` fits any number.

## Online-only warning (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- All four facts stay: the trash would download the files, so Cmdr offers only to delete the WHOLE selection, no copy
  lands in the trash but the service keeps its own (❌ don't play it down), and the ways out. The `<strong>` spans stay,
  and the quoted `”Radera”` is `fileOperations.delete.confirmDelete` verbatim.
- Prose says `bara online` (the everyday form); `endast online` is the ruled name. selection → `markering`, not `urval`.

## Look-alike names (`errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`, `fileOperations.transferProgress.lookAlikeHint`)

- Plain `stavar dem olika` / `lagrar dem med olika stavning`, no Unicode jargon as `@key` asks;
  `stora och små bokstäver` (Finder) over technical `skiftlägeskänslig`. The button label is the subject unquoted:
  `Skriv över ersätter …`.

## Escape leaves full screen (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `lämna helskärmsläge` (AppKit), definite for the state left (`tog Cmdr ur helskärmsläget`). The key is `Escape` in
  sentences (AppKit), `ESC` only on the keycap. `switchLabel` and the settings label share one value.
