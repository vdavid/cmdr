# nl decisions

The rationale journal behind `terms.json`: why a term won, which catalog keys a ruling shaped, and the incidents that
encode a constraint. Not read by default; `pnpm i18n:brief` pulls the sections whose heading cites a batch's keys, so
keep citing keys in backticks in every heading. The term rulings themselves live in `terms.json` (one entry per concept
from `../concepts.json`); open questions for a native reviewer live in `review-queue.md`. Style and voice: `style.md`.

## Cross-file reconciliation

Drift the parallel per-file passes left behind, found and fixed in a whole-locale reconciliation:

- **Ellipsis style follows the EN source per key.** EN mixes `…` and `...` deliberately; match it. The
  ai/licensing/settings/viewer passes had silently converted 32 EN `...` to `…`; reverted. Don't normalize ellipses to
  one glyph.
- **Quoted UI strings inside running text use single curly quotes `‘…’`**, never straight `"…"`. The
  commands/fileExplorer/settings passes left 15 values with EN's straight quotes; converted.
- **Brand tokens stay verbatim, so avoid the Dutch genitive-s on them.** `errorReporter.dialog.description` had "Cmdrs
  recente logbestanden" (the `desktop-i18n-dont-translate` check reads "Cmdrs" as a dropped "Cmdr"); rephrased to "de
  recente logbestanden van Cmdr". Prefer `van X` over `X's`/`Xs` for brand names.
- **Settings-section references match across files**: "Instellingen > AI" ↔ `settings.section.ai`; "Instellingen >
  Sneltoetsen" ↔ `settings.section.keyboardShortcuts`; "Instellingen > Updates" (crashReporter) vs "Instellingen >
  Updates en privacy" (whatsNew) are deliberately distinct, each tracking its own EN source. Menu-path separators (`>`
  vs `→`) mirror EN per key.

## Instellingssecties en kaartnamen (`settings.section.*`)

Keep these consistent across every file that names them:

- Appearance → Weergave; Behavior → Gedrag; File operations → Bestandsbewerkingen; File system watching →
  Bestandssysteem volgen (tentative); Search → Zoeken; File systems → Bestandssystemen; SMB/Network shares →
  SMB-/netwerkshares; MTP (Android/Kindle/cameras) → MTP (Android/Kindle/camera's); Viewer → Weergavevenster
  (tentative); Developer → Ontwikkelaar; Logging → Logboek; Updates & privacy → Updates en privacy; Advanced →
  Geavanceerd; Keyboard shortcuts → Sneltoetsen; License → Licentie; Colors and formats → Kleuren en notaties; Zoom and
  density → Zoom en dichtheid; File and folder sizes → Bestands- en mapgroottes; Listing → Lijst (tentative); Navigation
  (card) → Navigatie.
- **Navigation & file ops (short sidebar section) → `Navigatie en bewerkingen`**
  (`settings.section.navigationAndFileOps`): `bewerkingen` mirrors EN's casual clip of "operations" to "ops"; the
  sibling card keeps the full `Bestandsbewerkingen`. "&" → "en", like `Updates en privacy`.
  `settings.summary.navigationAndFileOps` says "het wijzigen van bestandsnamen" (macOS rename wording).
- **Double-click the pane background** (`settings.behavior.doubleClickPaneNavigatesToParent.*`): "Dubbelklik op de
  paneelachtergrond om naar de bovenliggende map te gaan"; the description "Dat is de lege ruimte rondom de
  bestandenlijst, geen bestandsrij."
- Settings toggle LABELS are infinitive-final ("Statusmarkeringen op afbeeldingsbestanden tonen", "Inhoud van
  afbeeldingen indexeren"); their DESCRIPTIONS use the imperative ("Voeg … een kleine markering toe …").

## De wachtrijmelding telt mee (`fileOperations.transferProgress.queuedToast`, `.queuedToastCount`)

EN puts the count phrase ("1 operation") first; Dutch needs the verb to agree with the count, so the count FRAGMENT
carries the finite verb ("gaat # bewerking" / "gaan # bewerkingen") and the host sentence wraps it: "Er {countText} deze
voor, dus deze wacht op zijn beurt." Renders "Er gaat 1 bewerking deze voor" / "Er gaan 3 bewerkingen deze voor". ⚠️ The
two keys are ONE unit: never re-translate either half alone, or the verb stops agreeing.

## Voortgang en uitkomst van kopiëren en verplaatsen (`transfer.split.*`, `transfer.fileOnly.*`, `fileOperations.transferProgress.titleActive`, `queue.row.label`)

- `transfer.split.clean`/`.skipped` put the verb sentence-final ("{phrase} gekopieerd/verplaatst"), natural Dutch word
  order against EN's leading "Copied/Moved {phrase}"; `{phrase}` keeps its grammatical slot.
- "-ing" progress titles → `Bezig met …` (`Bezig met kopiëren/verplaatsen/verwijderen/comprimeren/terugdraaien`); a
  short inline progress word with an ellipsis uses the bare infinitive (`Zoeken…`, `Verwijderen…`, `Doorzoeken…`). The
  trash arm is `Naar prullenmand verplaatsen`.
- `queue.row.label` arms for rename / create folder / create file: `Bezig met naam wijzigen` / `Bezig met map aanmaken`
  / `Bezig met bestand aanmaken` (the "Bezig met [infinitief]" family). `queue.failureToast.title` strips "Bezig met"
  and appends `niet voltooid`; the two keys move together.
- Destination-folder warning (`fileOperations.transferDialog.*`): "Deze map bestaat nog niet. Cmdr maakt hem aan tijdens
  het {kopiëren/verplaatsen}." `hem` for the de-word `map`; the operation verb as `het kopiëren`/`het verplaatsen`.
- "and N more files/items" (trailing list line) → `en nog {countText} bestand(en)` / `onderdeel(en)`: `nog` carries the
  more-sense.
- A too-large file (`errors.write.filesTooLargeForFilesystem.*`): "te groot" for size ("te lang" is for names); "die
  heeft zo'n beperking niet" for "has no such limit".

## Archieven bladeren en bewerken (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `settings.archives.*`)

- Browsing is distinct from opening: "Blader als een map" vs "Open met standaardapp".
- `settings.archives.opt.open` is `Open`, identical to EN and justified; macOS uses `Open` as the imperative.
- Deleting from a zip has no trash: "worden definitief uit de zip verwijderd".
- Read-only titles close into one compound: `Alleen-lezenarchief`, `Alleen-lezenapparaat`, `Alleen-lezenvolume`,
  `Alleen-lezenmap`, `Alleen-lezenbron`. The adjective stays open as a predicate or after a comma ("is alleen-lezen",
  "een tijdelijke, alleen-lezen kopie"), and `Alleen-lezen git-portaal` stays open to avoid a double hyphen.
- The settings blurb's generic "each format" → `elk formaat`; `structuur` is reserved for the compression format
  (`archiefstructuur`).

## Klembordinhoud als bestand plakken (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- Toast order follows the sibling `clipboard.copied` (object, then participle): `… als {filename} geplakt`.
- Compounds on `Klembord`: `Klembordinhoud`, `Klembordafbeelding`, `Klembordtekst`, and `Klembord-PDF` (hyphen before an
  acronym). The full compound sits INSIDE each select branch so PDF keeps its hyphen; each branch starts the sentence,
  so all three capitalize.
- Radio options are infinitives, parallel to each other: `Niets doen` (not the imperative `Doe niets`),
  `Bestand aanmaken`, `Aanmaken en naam wijzigen`.

## Het archiefwachtwoord (`fileOperations.archivePassword.*`)

- The body says "… is beveiligd met een wachtwoord." (Total and Double Commander nl phrasing); the input's aria label
  compounds to `Archiefwachtwoord`; the button and verb are `Ontgrendelen` / "om het te ontgrendelen" (macOS AppKit).
- `errors.listing.archiveNeedsPassword.explanation` keeps the pair apart: the archive is `beveiligd`, its contents stay
  `vergrendeld` until you `ontgrendelt` it. The Finder Locked flag is a different state (`locked`: `beveiligd`).

## Comprimeren (`commands.fileCompress.*`, `fileOperations.transferDialog.*`, `settings.archives.compressionLevel.*`)

- `Comprimeer` for `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and both title-verb branches;
  `scanTitleCompress` = "Controleren voor het comprimeren...". `.zip` in straight double quotes.
- Slider ends: `Sneller` (level 1) marks quicker packing, not app speed; `Kleiner` (level 9) marks the smaller output
  file (TC nl "snelste compressie (1)" / "maximale compressie").

## Bewerkingenlogboek (`operationLog.*`, `commands.logOperationLog.*`)

- Rename summary: `Naam van {countText} onderdeel gewijzigd` / `Namen van {countText} onderdelen gewijzigd` (macOS "De
  naam van het onderdeel ... gewijzigd"); `dialog.empty` is reordered so "wijzig de naam van iets" keeps its object.
- Lifecycle words reuse `queue.row.status`: Queued → `Wachten`, Running → `Bezig`, Done → `Gereed`, Didn't finish →
  `Niet voltooid`, Canceled → `Geannuleerd`; per item Skipped → `Overgeslagen`.
- Initiators: You → `Jij` (contrastive standalone), AI client → `AI-client`, Agent → `Agent` (justified as same as EN).
- recorded → `vastgelegd` (tentative); can (not) roll back → `Terug te draaien` / `Niet terug te draaien`.

## Ask Cmdr: chat, hulpmiddelen en kosten (`askCmdr.*`, `settings.askCmdr.*`, `commands.askCmdrToggle.*`)

- The tool-status `doing`/`done` pairs (`askCmdr.tool.*`) have no pile precedent: present tense without subject for
  doing ("Controleert wat je bekijkt"), participle-led for done ("Grootste mappen gevonden"), a distinct verb per tool
  so the pairs stay apart.
- `askCmdr.error.budgetExhausted`: the literal word "budget" never appears; `limiet`.
- Brand + possessive ("Cmdr's AI") → `de AI van Cmdr`; the Ask Cmdr panel name hyphenates before a Dutch head
  (`Ask Cmdr-model`, `Ask Cmdr-chats`).
- `askCmdr.cost.tokens` renders byte-identical to English (justified): Dutch shares EN's one/other categories and
  `token` is the kept loanword.
- `askCmdr.composer.dropHint` "Drop to attach" → "Zet hier neer om bij te voegen".

## Afbeeldingen indexeren en doorzoeken (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.*`)

- **Register split** mirroring EN: technical labels say `afbeelding` ("Afbeeldingen indexeren", `afbeeldingsbestand`);
  warm user-facing rows about a network photo archive say `foto''s` (ICU-doubled).
- The feature named in prose is `het doorzoeken van afbeeldingen` ("Het doorzoeken van afbeeldingen staat uit voor deze
  schijf."); "Indexed for image search" → "Geïndexeerd voor het doorzoeken van afbeeldingen".
- In-progress status "Indexing images" → `Afbeeldingen worden geïndexeerd` (passive progress, like "wordt gedownload");
  a bare `Afbeeldingen indexeren` would read as the Settings label. "Indexing now" (badge tooltip and progress heading,
  one source hash) → `Wordt nu geïndexeerd`; pending → `Wacht op indexering`; failed → `Kon niet worden geïndexeerd`;
  still working → `nog bezig`.
- Card titles: `Indexeren inschakelen`, `Mappen om te indexeren` (friendlier than "Te indexeren mappen").
- Semantic search: toggle `Foto''s op beschrijving zoeken`; delete-confirm title `het model voor semantisch zoeken` (not
  a compound); reassurance `zoeken op trefwoord` / `zoeken op tag`; gentle failure "kon nu even niet worden verwijderd",
  "Probeer het zo opnieuw". `clip.notSupported` = "Zoeken op beschrijving vereist een Mac met Apple silicon."
- A network drive comes back: "gaat verder zodra deze schijf opnieuw verbinding maakt"; drops off mid-pass:
  `wordt losgekoppeld` / `is losgekoppeld`.
- `fileExplorer.imageIndex.drive.ariaLabel` keeps the feature phrase ("Status van het doorzoeken van afbeeldingen voor
  deze schijf"); the double `van` is heavy but clear.

## Indexeren: runsoorten en resterende tijd (`indexing.run.*`, `indexing.eta.*`, `indexing.enrich.queued`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`)

- Run-kind headers: `Eerste volledige scan`, `Volledige herscan` (herscan tentative), `Snelle update`; the rescan toasts
  spell the verb as "de schijf opnieuw doorzoeken".
- Hour-scale ETA leads with `nog` and keeps `uur` invariant: "nog 1 uur 24 minuten", "nog 20 uur".
- "The drive scan" in prose → `het doorzoeken van de schijf` / `de schijf wordt nog doorzocht` (`indexing.enrich.queued`
  = "Het indexeren van afbeeldingen begint na het doorzoeken van de schijf").

## Naamwijzigingen beoordelen (`askCmdr.renameReview.*`, `askCmdr.renameUndo.*`)

- Rename as a noun is `naamwijziging` (compound `naamwijzigingsplan`, `-s-` before a noun per MS
  "naamwijzigingsvoorstellen"). `hernoemen` has zero hits in `nl/macOS/` and is Tier-3-only; the catalog carries no
  `hernoem*` form.
- `askCmdr.renameReview.rename` "Rename {count} files" → `Wijzig # bestandsnaam` / `Wijzig # bestandsnamen`, short
  enough for the primary button; `askCmdr.renameReview.title` → `Naamwijzigingen beoordelen` (the modal supplies
  "file").
- Rename cycle → `cyclus van naamwijzigingen`; "while rotating these files" → `terwijl deze bestanden van naam wisselen`
  (`roteren` reads mechanical); the badge stays `(cyclus)`.
- In error prose, "moved, renamed, or deleted" → "verplaatst, van naam veranderd of verwijderd"; "rename one first" →
  "wijzig eerst de naam van een ervan".
- **Recent-past events in status or tooltip prose take the perfect**, not the simple past (macOS "is mogelijk verplaatst
  of verwijderd"): `driveIndex.tooltipCoalesced*` reads "macOS is … het spoor … kwijtgeraakt".

## Het verwijdervenster: Van, Naar en de prullenmandschakelaar (`fileOperations.delete.trashSwitch`, `.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`, `.targetGroupTitle`)

- The switch and its confirm button read as one pair: `Naar prullenmand`, identical to `transferDialog.titleVerbOnly`'s
  trash arm. Finder's fuller "Verplaats naar prullenmand" stays the sentence form.
- `Van` / `Naar` headings (Total Commander and Double Commander ship this exact pair in the same dialog); the nouns bron
  / bestemming stay for the controls. "Naar" doubles as the trash preposition, but the two live in different dialogs.

## Het indexeren van schijven staat uit (`fileExplorer.navigation.driveIndex.refusedIndexingOff`, `.tooltipIndexingOff`, `.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `.overriddenBadge`)

- **"drive indexing" as a subject or object in prose → `het indexeren van schijven`**, never the bare label
  `Schijf indexeren`, which garden-paths as an imperative ("index the drive!") and drops the article a nominalized
  infinitive wants. The plural carries the global reading the master switch needs, and the het-word gives "Zet het aan"
  a real antecedent. KDE Dolphin corroborates ("Indexeren van bestanden").
- A navigation path still quotes the label verbatim: `Zet het aan bij Indexeren > Schijf indexeren.`
- Terse slots (badges, aria, internal descriptions) take the compound: `schijfindexering`, `beeldindexering`,
  `achtergrondindexering` (`indexing.status.ariaLabel` "Status van schijfindexering"; MS "inhoudsindexering").
- `overriddenBadge` → `Uit met schijfindexering` (comitative `met`, badge-short). "stays unindexed" →
  `wordt niet geïndexeerd`; "keeps its own on or off choice" → `onthoudt of hij aan of uit staat` (`schijf` is a
  de-word, `hij`); "picks up where it left off" → `pakt … de draad weer op`.
- `fileExplorer.navigation.driveIndex.tooltipDisabled` carries the article too ("Het indexeren staat uit …") so the two
  tooltips that alternate on the same dot stay parallel.

## Schijfindex: de wijzigingscontrole

- **"Checking for changes" (run-kind header) → `Controleren op wijzigingen`** · Nautilus NL models the exact
  construction ("Unable to poll “%s” for media changes" → "Kan ‘%s’ niet **controleren op** mediawijzigingen"), and
  `wijzigingen` is catalog-settled (`Recente wijzigingen inhalen`) · high.
- **"Update the file list" → `Bestandenlijst bijwerken`** · composed from the settled siblings `Bestandenlijst opslaan`
  - `Index bijwerken` · high.
- **"the check running right now" → `de controle die nu bezig is`** · reuses `controle` as this catalog's settled word
  for a full check (`tooltipCoalesced`: "de volgende volledige controle van Cmdr") and that string's closing
  `zet dat weer recht` · high.

## Vastgelopen overdracht: het stall-bericht

Seven keys for the stalled-transfer notice (`fileOperations.transferProgress.close`/`.stallNotice`/
`.stallWaitingDestination`/`.stallWaitingSource`/`.stallUnknown`/`.stallInFlight`/`.stallLogHint`); mined
`_ignored/i18n/nl/`, 2026-07-31.

- progress (of a transfer) → `voortgang` · macOS AppKit/Finder ("Progress"→"Voortgang", "Toon kopieervoortgang"),
  Microsoft terminology ("Progress"→"Voortgang", NLD/BEL), Thunar ("File Operation Progress"→"Voortgang van
  bestandbewerking"), Double Commander ("Show operations progress"→"Toon voortgang van bewerkingen); already in-catalog
  as `Voortgang grootte` / `Voortgang bestanden` · high
- "No progress for {duration}" → `Al {duration} geen voortgang` · the "al X geen Y" construction is the natural Dutch
  for an elapsed-since-anything-happened line; the literal `Geen voortgang gedurende …` reads bureaucratic. One key,
  `stallNotice`, feeds both the progress dialog and the queue row, and its value carries no final period, matching
  English · high on the term, `tentative` on the construction
- respond (a device/share answering) → `reageren` · macOS AppKit ("… did not respond to the request for services"→"…
  reageert niet op het verzoek om voorzieningen"). Microsoft's `beantwoorden` is the reply-to-a-message sense, not this
  one; macOS is Tier 1 · high
- "Waiting for X to respond." → `Wachten tot X reageert.` · macOS Finder's own two waiting shapes are
  `Wachten op <noun>` ("Waiting for disc drive…"→"Wachten op schijfeenheid…") and `Wachten tot <clause>` ("Waiting for
  transfer with '^0' to complete…"→"Wachten tot overdracht met '^0' is voltooid…"); the clause form fits a verb like
  `reageert` · high
- source (the side being read FROM) → `bron` · Double Commander ("Source"→"Bron", "Waiting for access to file
  source"→"Wachtend op toegang tot bestandsbron"), Microsoft terminology ("source"→"bron", NLD/BEL) · high
- destination (the side being written TO) → `bestemming` · `terms.json` (macOS "at Destination"→"op bestemming");
  Microsoft's `doel` is the Windows term, macOS wins per term-choice principle 2 · high
- "has stopped moving" (a stalled transfer) → `komt niet meer vooruit` · no pile source names a stall; chose the
  unambiguous "makes no more headway" over `ligt stil` / `staat stil`, which sit too close to the neighbouring
  `Gepauzeerd` state in the same dialog · tentative
- "partly written" (bytes already on disk) → `gedeeltelijk geschreven` · macOS Finder models the plain verb for data
  landing on a device ("Er zijn gegevens naar deze schijf geschreven, maar de bewerking is niet voltooid."); the
  in-catalog `gedeeltelijke doelbestanden` (`rollbackTooltip`) confirms `gedeeltelijk` · high
- the log (Cmdr's on-disk log file, `stallLogHint`) → `het logbestand` · deliberately NOT the generic `logboek`: this
  catalog already owns `Bewerkingenlogboek` as a FEATURE name, and `Het logboek heeft de details` would point at the
  wrong thing. `logbestand` is what Settings shows the user (`settings.logging.openLogFile` = "Open logbestand"). The
  sentence shape mirrors the in-catalog `Het bewerkingenlogboek heeft de details.` · high
- close (dialog button that leaves the work running) → `Sluit` · `terms.json`; the catalog-wide rendering of "Close" (10
  keys incl. the sibling `fileOperations.errorDialog.close`), and clearly distinct from `Annuleer` next to it · high

Notes:

- `stallInFlight` splits only the noun + copula across the plural branches
  (`{count, plural, one {# bestand is} other {# bestanden zijn}} nog open en misschien al gedeeltelijk geschreven.`), so
  the shared tail carries both predicates on one copula. Same technique as `askCmdr.renameUndo.*`'s
  `{count, plural, one {het is} other {ze zijn}}`. Dutch CLDR categories are `one` / `other`.
- `stallUnknown` refers to the transfer as `hem` (de-word `overdracht`), matching `queuedToast` / `backgroundedToast`
  ("Vind hem in de bewerkingenwachtrij"), and reuses `laat … op de achtergrond doorlopen` verbatim from `queueTooltip`.
  The comma before `of` follows both the EN source and the in-catalog habit ("Probeer opnieuw, of verbreek de
  verbinding").

## Gekopieerd pad: de klembordbevestiging (`fileExplorer.clipboard.copiedPath`)

Eén sleutel: de regel van de informatiemelding na ⌘⌥C. Het pad staat eronder op een eigen regel in een
vaste-breedtelettertype, dus het is GEEN plaatshouder in de zin: de zin eindigt op een dubbele punt en moet zonder het
pad kloppen.

- **"Copied the path, it's now on your clipboard:" → `Pad gekopieerd, het staat nu op het klembord:`** · hergebruikt
  `clipboard → klembord` en `path → pad` uit `terms.json` (macOS AppKit) · high. Object-dan-voltooid-deelwoord volgt de
  zustermeldingen (`{countText} onderdelen gekopieerd`), en `op het klembord` matcht `clipboard.empty` ("Geen bestanden
  op het klembord"). Geen bezittelijk voornaamwoord: er is er maar één.

## Operation queue: de hernoeming van Overdrachtswachtrij

The English window widened from **"Transfer queue"** to **"Operation queue"**: it lists deletes, trashes, renames, and
folder/file creations too, not only copies and moves, and "transfer" already means copy-or-move one level down (the
transfer progress dialog, the transfer driver). Fourteen `nl` keys were re-translated across `queue.json`,
`commands.json`, and `fileOperations.json`.

- **operation (the category: a copy, move, delete, trash, rename, folder/file creation, or archive edit) → `bewerking`
  (plural `bewerkingen`)** · macOS Finder/AppKit Tier 1 uses `bewerking` for exactly this concept ("… wordt gebruikt
  voor een andere bewerking, zoals het verplaatsen of kopiëren van een onderdeel of het legen van de prullenmand", "De
  Finder kan niet worden gestopt, omdat er nog bewerkingen worden uitgevoerd"); Microsoft terminology
  operation→bewerking (NLD/BEL, two entries); Double Commander "Show operations progress"→"Toon voortgang van
  bewerkingen" · high. It is also what this catalog already ships: `Bewerkingenlogboek`, `Bestandsbewerkingen`,
  `transferDialog.operationAria` "Bewerking", `operationLog.dialog.empty` "Nog geen bewerkingen".
- **operation queue (the window, the View menu item, and the command palette entry) → `Bewerkingenwachtrij`** ·
  `bewerkingen` (above) + `wachtrij` (Double Commander, Total Commander, Thunar, and Microsoft terminology
  queue→wachtrij; no macOS term) · high. One closed compound per the compound rule. **Linking morpheme `-en-`, matching
  the shipped sibling `Bewerkingenlogboek`**: the two sit next to each other in the same View menu block (queue = what
  runs now, log = what already ran) and have to read as a pair, and `-en-` is the plural-content link Dutch uses for a
  container of many things (`bestandenlijst`). Microsoft's own `-ing` + wachtrij compounds take `-s-`
  (`bezorgingswachtrij`, `faseringswachtrij`, and every `bewerkings-` compound: `bewerkingsmodus`, `bewerkingstijd`), so
  `Bewerkingswachtrij` is defensible too; the in-catalog pair wins per the "target locale's own catalog outranks the
  pile for the same concept" rule. Flagged below.
- **SUPERSEDES `Overdrachtswachtrij`** (transfer-queue pass, 2026-06-21) everywhere the WINDOW is named. `overdracht`
  keeps its narrower copy/move sense; see the amended row in that pass.
- Command label drops its old "Toon" prefix: `commands.queueShow.label` is now exactly `Bewerkingenwachtrij`, identical
  to `queue.windowTitle`, matching how the sibling `commands.logOperationLog.label` is exactly `Bewerkingenlogboek` ·
  high.
- Queue-row aria labels take `deze bewerking` (`Pauzeer/Hervat/Annuleer/Selecteer deze bewerking`), NOT
  `deze overdracht`: the row can be a delete or a rename. The progress dialog's own
  `transferProgress.pauseAria`/`.resumeAria` still say `deze overdracht`, because their English still says "transfer" ·
  high.

## De voortgangschip en het niet-voltooid-bericht

Nine new `queue.json` keys for two surfaces: the main window's ~80 px corner progress chip (`queue.chip.*`) and the
failure notice plus its dismissable queue rows (`queue.failureToast.*`, `queue.row.dismiss*`,
`queue.toolbar.dismissAll`). The window name and the head noun come from the operation-queue rename pass above; this
section records only what was new.

- **dismiss (remove a finished-with-a-problem row from a list; nothing is undone, retried, or deleted) → `Sluit`**
  (`queue.row.dismiss`), `Sluit deze bewerking` (`queue.row.dismissAria`, the family shape
  `Pauzeer/Hervat/Annuleer/Selecteer deze bewerking`), `Sluit alles` (`queue.toolbar.dismissAll`, the
  `<imperatief> alles` shape of `Pauzeer alles`) · high. The one-thing-one-name drift round below settled this on
  `Sluit`: `Wis` (Finder's "Wis menu") means erase, and the `@key` says nothing is deleted.
  - ❌ NOT `Verwijder uit lijst` (the settled `remove from list`): it leads with the delete verb on a row whose own
    label can be "Bezig met verwijderen", and it is far too long for a `size="mini"` row button. ❌ NOT `Verberg`:
    implies the row can come back, and it can't.
- **"Couldn''t finish X" (failure-notice headline) → `X niet voltooid`** · built on the row's own status word
  `Niet voltooid` (`queue.row.status` failed arm) so the toast, the row, and the chip say one thing. Each arm is the
  matching `queue.row.label` verb with "Bezig met " stripped, in the nominalized infinitive Dutch headlines take:
  `Kopiëren niet voltooid`, `Verplaatsen niet voltooid`, `Verwijderen niet voltooid`,
  `Naar prullenmand verplaatsen niet voltooid`, `Naam wijzigen niet voltooid`, `Map aanmaken niet voltooid`,
  `Bestand aanmaken niet voltooid`, `Archief bewerken niet voltooid`, `other` = the bare `Niet voltooid` · high. No
  `fout` / `mislukt` anywhere, per the voice rule.
- **"N operations couldn''t finish" (count headline) → `{countText} bewerking is` / `{countText} bewerkingen zijn` + the
  shared tail ` niet voltooid`** · the copula lives INSIDE the plural branches and the predicate is shared, per
  `style.md` § Plurals · high. Renders "1 bewerking is niet voltooid" / "3 bewerkingen zijn niet voltooid".
- **"Show in operation queue" (notice button) → `Toon in de bewerkingenwachtrij`** · `show → Toon` (macOS Finder) + the
  window name, lowercased mid-sentence with its article exactly as the catalog already writes it
  (`transferProgress.queueTooltip` "… in de bewerkingenwachtrij (F2)", `queuedToast` "Vind hem in de
  bewerkingenwachtrij") · high.
- **"Open the operation queue (to see why)" (the chip's promise) → `Open de bewerkingenwachtrij`
  (`… om te zien waarom`)** · `open → Open` (macOS Finder imperative) + the same article-and-lowercase treatment · high.
- **"percent", spelled out for screen readers → `procent`** · the Dutch word, which VoiceOver nl reads naturally; the
  `%` SIGN stays in the visual tooltip and takes NO space before it in Dutch (`{percentText}%`, matching the catalog's
  "Zoom naar 100%" and `lowDiskSpace` "{percentText}%") · high.
- **"items" in the chip tooltip → `onderdeel` / `onderdelen`** · the settled macOS Finder term (`terms.json`); covers
  files and folders alike · high.
- **"{duration} left" in the tooltip's trailing slot** arrives already rendered from
  `fileOperations.transferProgress.etaRemaining` = `nog {duration}`, so the settled "left → nog …" phrasing needs
  nothing here; the paused variant arrives as `Gepauzeerd` from `queue.row.status` · high.

**`queue.chip.tooltip` needed a word-order change English doesn't.** English hangs the count straight off the action
word ("Copying 214 items to Backup"). Dutch can't: `{label}` is a `queue.row.label` arm, and while "Bezig met kopiëren
van 214 onderdelen" would work, the trash arm ("Naar prullenmand verplaatsen") and the fallback ("Bezig") take no
`van`-object at all. So the count clause carries its OWN middle dot (` · {countText} onderdelen`) and reads as one more
fact on the line, while the destination clause stays attached to whatever precedes it (` naar {destination}`). That
clause is grammatical both after the count and directly after the label, and `destinationName()` returns `''` for
deletes and trashes, so "Naar prullenmand verplaatsen naar X" can't occur. Every optional clause keeps its own leading
space and its own separator, and both empty arms stay empty. The four combinations render:

- `Bezig met kopiëren · 3 onderdelen naar Backup · 42% · nog 1m 20s`
- `Bezig met kopiëren naar Backup · 42% · nog 1m 20s` (count 0)
- `Bezig met verwijderen · 3 onderdelen · 42% · nog 1m 20s` (no destination)
- `Bezig met verwijderen · 42% · nog 1m 20s` (count 0, no destination)

## Het losse conflictvenster (`fileOperations.operationConflict.*`)

Two keys for the standalone conflict prompt the main window hosts when a backgrounded operation hits a name clash: the
context line under the title `Bestand bestaat al`, and the quiet note under the buttons.

- **A destination clause on the `Bezig met <infinitief>` frame goes AFTER the infinitive: `Bezig met kopiëren naar X`**,
  never `Bezig met naar X kopiëren` · a PP extraposes past a Dutch bare infinitive while a direct object may not, and
  `queue.chip.tooltip` already puts its `naar {destination}` clause after the `queue.row.label` arm (there as its own
  dot-separated fact) · high. ⚠️ macOS Finder's own progress titles use a DIFFERENT frame, verb-final with the
  destination in the middle (`nl/macOS/Finder/LocalizableMerged.json`: `CP4_V1` "Copying “^1” to “^2”" → "'^1' naar '^2'
  kopiëren", `MV4_V1` → "'^1' naar '^2' verplaatsen"). Cmdr does not follow it here: the whole job of this line is to
  let the user match the prompt to a row in the `Bewerkingenwachtrij`, so the row's own `Bezig met …` head has to
  survive verbatim. Finder's form stays the model only for keys that have no queue-row sibling.
- **A direct object DOES sit before the infinitive**, so the archive_edit arm that names the archive is
  `Bezig met {destination} bewerken` (the sibling `queue.row.label` arm with its generic `archief` swapped for the name)
  · high. The no-destination arm keeps the sibling verbatim, `Bezig met archief bewerken`; English's indefinite article
  ("Editing an archive") is dropped, because `Bezig met een archief bewerken` reads heavier than the label the user just
  saw on the queue row.
- **"Working in {destination}" → `Bezig in {destination}`** · the settled `other` arm `Bezig` (queue.row.status running,
  queue.row.label fallback) plus a plain locative `in`, which is what Dutch takes for being at work inside a folder ·
  high.
- **"until you answer" → `totdat je antwoordt`** · `totdat je …` is this catalog's settled shape for the construction
  (`errors.listing.archiveNeedsPassword.explanation` "totdat je het ontgrendelt", `indexing.staleDialog.body` "totdat je
  opnieuw doorzoekt", `settings.mediaIndex.reclaim.line`), and `totdat` is attested in `nl/macOS/` · high. Microsoft's
  `answer` (Verb) → `beantwoorden` is transitive and would need an object; the intransitive `antwoorden` fits a prompt
  waiting on the user. ❌ NOT `reageren`, which this catalog reserves for a device or share answering
  (`transferProgress.stallWaiting*` "Wachten tot … reageert").
- **"Everything else is paused" → `Al het andere is gepauzeerd`** · `gepauzeerd` is the queue's own state word
  (`queue.row.status` paused arm, `transferProgress.titlePaused`), and macOS Finder models the exact copula (`NE110`
  "Copying “^0” has paused" → "Kopiëren van '^0' is gepauzeerd") · high.
- `Bezig in {destination}` is the terse fallback arm; the fuller `Bezig in de map {destination}` is wrong whenever the
  operand isn't a folder, so the short form stands.

## De knop voor een lege wachtrij: "Background"

Two `fileOperations.transferProgress` keys: the progress dialog's primary button when the operation queue is EMPTY
(`background`), plus its screen-reader name (`backgroundAria`). Same button as `queue` / `queueAria`, other state: with
nothing to queue behind, English names the act instead of the destination. "Background" is a VERB there.

- **"Background" (imperative button, send this running operation out of sight) → `Op de achtergrond`** · the settled
  in-catalog sense-term (transfer-queue pass: `background (run in the ~) → op de achtergrond`, Double Commander "Work in
  background" → "Werk op de achtergrond"), now carrying a whole button · high.
  - ❌ NOT the bare `Achtergrond`, even though Total Commander nl ships exactly that on this exact button (`WCMD.LNG`
    `{COMMON}` `4004="&Achtergrond"`, right next to `4005="Wachtrij"`, the pair Cmdr's two states mirror). macOS Dutch
    (Tier 1) uses `Achtergrond` ONLY for the visual backdrop ("Achtergrond:", "Wijzig achtergrond…",
    "achtergrondkleur"), so a lone `Achtergrond` on a progress dialog reads as a picture-or-color label, not as
    something you do to a transfer. TC's own siblings split the same way: Swedish took `I bakgrunden` and Hungarian
    `Háttérben`, both the "in the background" phrase rather than the noun; `Op de achtergrond` is that shape in Dutch.
  - ❌ NOT `Naar achtergrond` (the `Naar prullenmand` directional shape): grammatical, but "op de achtergrond" is the
    settled collocation for RUNNING there, while "naar de achtergrond" suggests moving a window behind another.
  - Dutch has no verb for "to background" (no `achtergronden`), so the bare-stem imperative button rule can't apply
    here; the prepositional phrase is the closest thing to a command, exactly as in the in-catalog sentences "laat hem
    op de achtergrond doorlopen" (`stallUnknown`) and "Laat dit op de achtergrond doorlopen" (`queueTooltip`).
- **"Keep this running in the background" (aria) → `Op de achtergrond laten doorlopen`** · `doorlopen` is the catalog's
  settled verb for an operation that keeps running (`queueTooltip`, `stallUnknown`, `backgroundedToast` "Loopt nog op de
  achtergrond"), and the infinitive-final shape matches the sibling `queueAria` "Naar de bewerkingenwachtrij sturen" ·
  high.
  - **WCAG 2.5.3 (Label in Name) containment is EXACT here**: the aria begins with the visible label verbatim,
    `Op de achtergrond` ⊂ `Op de achtergrond laten doorlopen`, capital included. Better than English, which only manages
    case-insensitive containment. ⚠️ The two keys are ONE unit: if the label is ever re-worded, the aria has to be
    re-shaped so it still opens with the label verbatim.
  - The imperative alternative `Laat dit op de achtergrond doorlopen` (the tooltip's first clause word for word) was
    passed over: it only contains the label case-insensitively, and it breaks the aria-register parallel with
    `queueAria`.

## Het stoppoortje: afsluiten terwijl er nog werk loopt (`main.quit.*`)

Seven `main.json` keys for the modal Cmdr raises when the user quits (⌘Q, the menu, or closing the main window) while a
copy, move, delete, trash, or archive edit is still running: a question title, a reassuring body, a list heading, a live
countdown plus its screen-reader name, and the two buttons. The head noun `bewerking` and the queue's own verb family
come from the operation-queue rename pass above; this section records what was new.

- **"Quit" (the app stopping, in a sentence) → `stoppen`; the imperative button → `Stop`** · macOS Dutch Tier 1 uses
  `stoppen`, NOT `afsluiten`: Finder's `A17` "The Finder can't quit because some operations are still in progress." →
  "De Finder kan niet worden gestopt, omdat er nog bewerkingen worden uitgevoerd.", `A19` (singular) → "… omdat er nog
  een bewerking wordt uitgevoerd …", AppKit "Quit Anyway" → "Stop toch", the menu "Stop Finder". Already `terms.json`'s
  settled `quit (app) → Stop` · high.
- **"Quit while N operations are running?" (title) →
  `Stoppen terwijl er nog {countText} bewerkingen worden uitgevoerd?`** · this is macOS `A17`/`A19` almost word for
  word, with Finder's own `er nog … wordt/worden uitgevoerd` frame carrying the count · high. The infinitive `Stoppen`
  opens it, the terse question shape Dutch uses for a yes/no dialog title (Finder's own `Wil je …?` frame needs an
  object and would stretch the line).
  - Only the noun plus its finite verb sits inside the plural branches
    (`{count, plural, one {een bewerking wordt} other {{countText} bewerkingen worden}}`), per `style.md` § Plurals; the
    shared tail carries `uitgevoerd?`. Renders "Stoppen terwijl er nog een bewerking wordt uitgevoerd?" / "… nog 3
    bewerkingen worden uitgevoerd?". Dutch CLDR categories are `one` / `other`.
  - The `one` arm takes the indefinite `een bewerking` (Finder `A19`), not `{countText}`: English does the same, and "1
    bewerking" would read like a tally on a title line.
  - Total Commander nl ships the same dialog and independently confirms both the noun and the verb: `WCMD.LNG.utf8`
    `1237="WAARSCHUWING: %i bewerking(en) actief op achtergrond!\nToch stoppen?"`.
- **"anything still being written" → `Alles wat nog wordt geschreven`** · **the body must stay number-neutral**: one
  operation writes several files at once and several operations can run at once, so the old fronted
  `Alleen het onderdeel dat …` states something false · high. `Alles wat` scopes it without a numeral and mirrors the
  opening `Wat al klaar is`.
- **"half-written" → `gedeeltelijk geschreven`** · reuses the settled `partly written → gedeeltelijk geschreven`
  (stalled-transfer pass), so the quit dialog and the stall dialog describe the same leftover the same way · high. It
  attaches to the free relative `wat … achterblijft`, not to a definite `het bestand`, for the same number-neutral
  reason.
- **"clears away" (deletes the leftover so it can't look complete) → `opruimen` (`ruimt … op`)** · plain Dutch for
  tidying something away, and deliberately NOT `verwijdert`: the dialog sits above a queue whose rows can literally say
  `Bezig met verwijderen`, and a second "verwijder" in the reassurance would read as more deleting rather than as
  cleanup · high on the sense, `tentative` on the word (no pile string names this act).
  - The body fronts that clause (`… en wat gedeeltelijk geschreven achterblijft, ruimt Cmdr op.`) so the relative clause
    never lands between the object and the separable `op`. The SVO alternative "… ruimt Cmdr het bestand op dat
    achterblijft" garden-paths on `op dat`.
- **"Whatever''s finished stays done." → `Wat al klaar is, blijft klaar.`** · `klaar` is this catalog's plain
  finished-word (`indexing` "Bijna klaar"); `blijft staan` was rejected because for a delete the finished work is files
  GONE, and "blijft staan" would promise the opposite · high.
- **"Quitting in N seconds, so a restart or logout never waits on Cmdr." →
  `Over {secondsText} seconden stopt Cmdr vanzelf, zodat herstarten of uitloggen nooit hoeft te wachten.`** ·
  `over N seconden` is the standard Dutch "N seconds from now"; `seconde` / `seconden` plural per Nautilus nl
  (`%d seconds` → `%d seconde` / `%d seconden`) · high.
  - `vanzelf` ("of its own accord") carries the `@key`'s point that Cmdr stops without being asked again, and pairs the
    countdown with its aria label.
  - **restart → `herstarten`, log out → `uitloggen`** · macOS Dutch Tier 1 labels the two menu items `Herstart` and
    `Log uit`, and the catalog already ships `Herstart` (`terms.json`) and `inloggen`/`Log in`. Microsoft's
    `opnieuw opstarten` (restart, Verb) and `afmelden` (log out / sign out, Verb) are the Windows forms and lose per
    term-choice principle 2 · high.
  - Cmdr is named ONCE. English repeats the app as the thing not being waited on; a second `op Cmdr` in the same Dutch
    sentence reads clumsy, and with Cmdr as the subject of the main clause the referent of `nooit hoeft te wachten` is
    unambiguous.
  - Only `{secondsText} seconde` / `{secondsText} seconden` sits inside the branches; `Over` leads and the whole `zodat`
    clause is shared.
- **"Time until Cmdr quits on its own" (aria) → `Tijd totdat Cmdr vanzelf stopt`** · `totdat` is this catalog's settled
  until-clause word (conflict-prompt pass: "totdat je antwoordt"; `indexing.staleDialog.body`), and `vanzelf stopt`
  repeats the countdown's own words so the spoken label and the visible line agree · high. WCAG 2.5.3 does not bind
  here: the countdown region has no visible label to contain, only a live number.
- **"Keep working" (button that calls the quit off entirely) → `Werk door`** · the bare-stem imperative of the separable
  `doorwerken`, the same shape as macOS's `Ga door` (Continue) and Double Commander's `Werk op de achtergrond` ·
  `tentative` (no pile string carries this exact button).
  - ❌ NOT `Annuleer`: the queue rows and the progress dialog next to it use `Annuleer` for cancelling the OPERATIONS,
    which is the opposite outcome. ❌ NOT `Later` (the settled dismiss-for-now word, `updates.toast.later`): the
    countdown is deleted, not deferred. ❌ NOT `Behoud` (macOS's "Keep"), which is the keep-this-file sense.
  - `Stop niet` (macOS Finder `BN63` "Don't Stop" → "Stop niet") is the attested negative twin and would be defensible,
    but English deliberately frames this positively, and `Werk door` reads as the friendlier of the two.
- **"Quit now" (primary, destructive) → `Stop nu`** · `Stop` (above) plus `nu`, which does the same load-bearing work as
  English's "now": the app quits either way when the countdown ends, and this skips the wait · high.
- **"Still running" (heading over the operation rows) → `Nog bezig`** · `Bezig` is the queue's own running-state word
  (`queue.row.status` running arm, `queue.row.label` fallback), so the heading and the rows under it speak one
  vocabulary; `nog` carries "still" · high.
- No `fout` / `mislukt` anywhere in the seven values, per the voice rule. No apostrophes, so no ICU doubling was needed.
  No `sameAsSourceJustification`: all seven differ from English.

## Usage stats: "anonieme" dropped, "een willekeurige id" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `pseudoniem` / `gepseudonimiseerd` —
that jargon is exactly what the copy avoids.

- **usage stats → `gebruiksstatistieken`** · already settled above and in `onboarding.stepBeta.emailNote`; only the
  `anonieme` adjective was cut · high
- **a random id → `een willekeurige id`** · MS terminology gives BOTH sides here (random → `willekeurig`, identifier →
  `id`) · high. Lowercase `id` mid-sentence, as MS has it.
- **tied to → `gekoppeld aan`** · the catalog's own verb (`onboarding.stepBeta.emailNote` "nooit gekoppeld aan je
  gebruiksstatistieken") · high
- **The toggle label and the onboarding title are ONE English string**, so both now read `Gebruiksstatistieken sturen`.
  They had drifted apart (`Anonieme gebruiksstatistieken sturen` vs the imperative
  `Stuur anonieme gebruiksstatistieken`); a toggle label describes a setting, not a button, so the infinitive wins over
  the bare-stem imperative the button rule prescribes.

## De terugdraaibevestiging en de rij die op antwoord wacht (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`)

De knop `Terugdraaien` op een lopende kopie of verplaatsing vraagt nu eerst om bevestiging, en een rij in de
`Bewerkingenwachtrij` krijgt een eigen status wanneer die stilstaat omdat er in het hoofdvenster een vraag klaarstaat.

- **"Needs your answer" (rijstatus) → `Antwoord nodig`** · Double Commander nl heeft de enige directe pile-treffer op
  dit concept (`Waiting for user response` → `Wachtend op reactie van gebruiker`), maar die is hier onbruikbaar: hij
  begint met `Wacht…`, precies het woord van de `queued`-arm (`Wachten`) die de `@key` verbiedt te laten lijken, en
  `reactie` is in dit catalogusdeel gereserveerd voor een apparaat of share dat antwoordt (`stallWaiting*`) · high op
  `antwoord`, `tentative` op de vorm. `<Zelfstandig naamwoord> nodig` is de gangbare Nederlandse statusvorm (vgl. "actie
  nodig") en past in de smalle kolom naast `Gepauzeerd` en `Niet voltooid`.
- **`awaitingAnswerTooltip` → `Beantwoord de vraag in het hoofdvenster, dan loopt deze bewerking door.`** ·
  `beantwoorden` (Microsoft, `answer` Verb) mag hier wél, omdat er een object staat; het intransitieve `antwoorden` van
  `operationConflict.pausedNote` ("totdat je antwoordt") heeft er geen. `doorlopen` is het gevestigde werkwoord voor een
  bewerking die blijft lopen (`queueTooltip`, `backgroundedToast`), en `hoofdvenster` is al vastgelegd · high.
- **`rollbackConfirm.title` → `Deze bewerking terugdraaien?`** · de catalogus zet elke ja/nee-dialoogtitel in de
  infinitief (`AI-model verwijderen?`, `{hostName} uit de serverlijst verwijderen?`) · high.
- **`rollbackConfirm.body` →
  `Dit verwijdert elk bestand dat de bewerking tot nu toe heeft geschreven. Wat daarbij is vervangen, komt niet terug.`**
  · `geschreven` is het catalogus­woord voor een weggeschreven doelbestand (`stallInFlight`, `main.quit.body`),
  `tot nu toe` is de vaste weergave van "so far" (`queryUi.results.live.matchesSoFar`), en `vervangen` is macOS Tier 1
  voor `Replace` · high. De tweede zin gebruikt de vrije relatiefzin `Wat daarbij is vervangen` in plaats van een
  voornaamwoord: `de bewerking` is een de-woord, dus `het` zou fout zijn, en zo blijft de zin bovendien getal-neutraal.
- **`rollbackConfirm.keep` ("Keep them", het veilige antwoord) → `Behoud de bestanden`** · macOS Tier 1 `Keep` →
  `Behoud` (AppKit Revisions: `Behoud beide`, `Behoud alle`, Finder `Behoud origineel`, `Behoud gedeeltelijke kopie`) ·
  high. Het object wordt uitgeschreven in plaats van `Behoud ze`: de laatste zin van de body noemt de VERVANGEN
  bestanden, dus een voornaamwoord kan even naar de verkeerde verwijzen.
- **`rollbackConfirm.rollBack` → `Terugdraaien`** · exact de knop die het venster opende
  (`transferProgress.conflictRollback`), zoals de `@key` vraagt · high. Wijkt bewust af van de bare-stem-imperatiefregel
  voor knoppen (`Draai terug`): gelijkluidendheid met de openende knop weegt hier zwaarder.
- **`transferProgress.rollbackTooltip` (nieuw Engels: "Stop, and delete every file written so far") →
  `Stop en verwijder elk bestand dat tot nu toe is geschreven`** · `Stop` is het catalogus­woord voor het stoppen van
  lopend werk (`queryUi` "Stop met zoeken", macOS `Stop toch`) en houdt de tooltip weg van `Annuleer`, wat de `@key`
  juist verbiedt · high. Geen komma voor `en`, anders dan het Engels.
- **`transferProgress.foregroundBusyToast` (nieuw Engels: "Something else is open here. Close it, then bring this one
  up.") → `Hier is iets anders open. Sluit het en haal deze daarna naar voren.`** · het nieuwe Engels claimt bewust niet
  dat de blokkade een andere BEWERKING is (het kan ook een nieuwe-map-venster of een verwijderbevestiging zijn), dus de
  oude opening `Een andere bewerking …` was onwaar geworden · high. `het` verwijst naar het onzijdige `iets anders`,
  `deze` naar de bewerking (de-woord).

## De keten-hernoemtoast die meetelt (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

Eén toast die wordt herschreven zodra een tweede bestand zijn naam houdt: hij noemt het laatste bestand en telt de
eerdere. De broer-en-zus-sleutel `chainKeptOriginalName` (`{reason}. ‘{name}’ behoudt zijn naam.`) is het anker; deze
waarde moet dezelfde stem, dezelfde aanhalingstekens en hetzelfde werkwoord houden.

- **Waarde:
  `{reason}. ‘{name}’ behoudt zijn naam, net als {others, plural, one {één ander bestand} other {{othersText} andere bestanden}}.`**
- **"kept its name" → `behoudt zijn naam`** (tegenwoordige tijd) · overgenomen van de broer-en-zus-sleutel, en
  `behouden` is macOS Tier 1 voor `keep` (Finder `Behoud origineel`, `Behoud gedeeltelijke kopie`, AppKit `Keep` →
  `Behoud`) · high. `zijn` hoort bij het onzijdige `bestand`.
- **"and so did N other files" → `net als {othersText} andere bestanden`** · macOS Finder telt na-komende onderdelen
  precies zo: `MR101_V3` / `MR201_V3` / `PE106_V4` renderen "‘^1’ and ^0 other items" als `'^1' en ^0 andere onderdelen`
  (Tier 1 voor `N andere <meervoud>`; de sleutelfamilie staat beschreven in `docs/i18n/translation-learnings.md` §
  Reference-pile notes). `net als` draagt het werkwoordsecho van "and so did" dat een kaal `en … ook` mist; het staat
  als vergelijkend voegwoord in Nautilus nl ("net als deze") · high op de telformule, tentative op `net als`.
- **Enkelvoud → `één ander bestand`**, zonder `-e` · Apple schrijft `'^1' en ^0 ander onderdeel` (`MR101_V2`,
  `MR201_V2`): geen buigings-`e` bij een onzijdig zelfstandig naamwoord met onbepaald lidwoord. Het afwijkende
  `^0 andere onderdeel` in `PE106_V3` is een slip in Apple's eigen catalogus, niet de regel · high. De accenten op `één`
  markeren het telwoord; de catalogus doet dat al bij `driveIndex.tooltipCoalesced` (`one {één keer}`), dus het Engelse
  woordelijke "one" krijgt hier ook een uitgeschreven telwoord.

## De onbevestigde naamwijziging en de onbruikbare naam (`fileExplorer.rename.unconfirmed`/`unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

Een traag volume (netwerkshare, telefoon) antwoordt niet op tijd op een naamwijziging. De toast mag NOOIT zeggen dat het
bestand zijn naam heeft gehouden: dat is precies wat we niet weten. Daarmee staat dit paar tegenover
`chainKeptOriginalName*`, dat wél zeker weet dat de naam onveranderd is.

- **Waarden:
  `We konden de naamwijziging van ‘{name}’ niet bevestigen. Het volume is mogelijk traag, dus de naam is misschien toch gewijzigd.`
  en
  `We konden de naamwijzigingen van ‘{name}’ en {others, plural, one {één ander bestand} other {{othersText} andere bestanden}} niet bevestigen. Het volume is mogelijk traag, dus de namen zijn misschien toch gewijzigd.`**
- **"Couldn''t confirm …" → `We konden … niet bevestigen`** · de catalogus heeft dit patroon al twee keer voor precies
  dezelfde time-outsituatie (`fileOperations.mkdir.timeoutMessage` "We konden niet bevestigen dat de map is aangemaakt",
  `fileExplorer.pane.trashUnconfirmedToast`), en `bevestigen` is Microsoft Tier 2 voor `confirm` (`DUTCH.tbx`, nl
  `bevestigen`) plus macOS AppKit (`Bevestig`) · high. Het Engels heeft hier een naamwoordelijk object ("the rename of
  X") in plaats van een `dat`-zin, dus het object staat vooraan en het werkwoord achteraan.
- **rename (naamwoord) → `naamwijziging` / `naamwijzigingen`** · de gevestigde rij hierboven (Microsoft
  "naamwijzigingsvoorstellen"); NOOIT `hernoeming` · high. Het meervoud in de `AndOthers`-arm is correct in beide
  takken: daar staan altijd twee of meer naamwijzigingen.
- **"The volume may be slow" → `Het volume is mogelijk traag`** · macOS Finder `LA20` ("… may run very slowly" → "… zijn
  erg traag") is Tier 1 voor `slow` → `traag`, en `mkdir.timeoutMessage` zegt deze halve zin al woordelijk zo · high.
  Het `mogelijk` draagt de slag om de arm van het Engels: Cmdr weet niet eens zeker dát het volume traag is.
- **"the rename may still have gone through" → `de naam is misschien toch gewijzigd`** · exact de staartvorm van
  `mkdir.timeoutMessage` ("dus de map is misschien toch aangemaakt") · high. De staart herhaalt het werkwoord uit de
  gevestigde `naam wijzigen`, zodat de zin de daad noemt en niet de afloop claimt. Meervoud in de `AndOthers`-arm
  (`de namen zijn`), want daar gaat het altijd om meer dan één bestand.
- **`{othersText} andere bestanden` / `één ander bestand`** · woordelijk overgenomen van
  `chainKeptOriginalNameAndOthers` hierboven (Apple Tier 1 telformule); de twee ketentoasts moeten dezelfde telstaart
  hebben.
- **"That filename can''t be used" → `Deze bestandsnaam kan niet worden gebruikt`** (map: `Deze mapnaam …`) · macOS
  Finder `RN31` is de directe Tier 1-treffer: "The name '^0' can't be used." → "De naam '^0' kan niet worden gebruikt."
  (ook `NE74`, `RN5`, `RN23`) · high. `Deze` volgt het aanwijzende `That` van het Engels en de zusterregel `nameTooLong`
  (`Deze mapnaam is te lang`); geen punt op het eind, want de zin wordt ook ingevoegd vóór `‘{name}’ behoudt zijn naam.`

## Voorgestelde bewerkingen: het venster met wat Ask Cmdr voorstelt (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- ops (de door de agent voorgestelde bestandsbewerkingen) → `bewerkingen`; titel `Voorgestelde bewerkingen` · sluit aan
  op "File operations" → "Bestandsbewerkingen" · high
- approve → `Goedkeuren` · ms/standaard NL; gekozen boven het `Accepteer` van macOS, omdat de telvariant ("3 bestanden
  goedkeuren") toestemming geeft in plaats van iets aan te nemen · high
- reject → `Weigeren` · macOS Finder, het paar Accepteer/Weiger in het AirDrop-venster (Tier 1); infinitief, omdat de
  knoppen hier infinitief zijn · high
- "This can't be undone" → `Dit kun je niet ongedaan maken` · macOS Finder, woord voor woord (waarschuwing bij direct
  verwijderen) · high
- suggestion → `suggestie` · al in de catalogus · high

## Dupliceer: de opdracht die in dezelfde map kopieert (`commands.fileDuplicate.*`)

- **duplicate (opdracht die de selectie in haar eigen map kopieert) → `Dupliceer`** · macOS Finder `nl`, menu 'Archief >
  Dupliceer' (`N154`), plus 'Dupliceer onderdelen' en 'Dupliceert onderdelen op huidige locatie' (gecontroleerd op macOS
  26.6.1, `Finder.app/Contents/Resources/nl.lproj`, 2026-08-19) · high. Gebiedende wijs, net als de zusters `Kopieer`
  (F5) en `Verplaats` (F6).
- **'Make a copy of the selected files in the same folder' →
  `Maak een kopie van de geselecteerde bestanden in dezelfde map`** · gebiedende wijs, zoals de naburige beschrijvingen
  ('Kopieer geselecteerde bestanden…'); 'dezelfde map' is de map waar de bestanden al staan · high.

## Native menu's: menubalk, contextmenu's, venstertitels (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Bronnen voor deze hele groep: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/nl.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) is Tier 1 en beslist bijna alles; de Engelse kant staat in `en_GB.lproj`, omdat
`Base.lproj` alleen gecompileerde nibs bevat. Safari 26 (`MainMenu.strings`) levert de tabblad-woorden, de
Microsoft-terminologie wat Apple niet benoemt. RAW-familie: **enkele apostroffen**, een `''` zou in het menu dubbel
verschijnen.

- **Menubalk → `Archief`, `Wijzig`, `Weergave`, `Ga`, `Venster`, `Help`, `Voorzieningen`** · macOS Finder en Safari `nl`
  · high. Niet „Bestand” en niet „Diensten”: Apple gebruikt al jaren `Archief` en `Voorzieningen`.
- **Select-menu (bestandsselectie) → `Selecteer`** · bare-stem imperatief volgens `style.md`, met Nautilus/Dolphin `nl`
  („Selecteren”) als bron voor het werkwoord · high.
- **Quick Look → `Geef snel weer`** · macOS Finder (`TL14`) · high. Apple vertaalt deze functienaam, dus hij staat NIET
  op de niet-vertalen-lijst.
- **Get Info → `Toon info`, Enclosing Folder → `Bovenliggende map`, Go > Home → `Thuismap`, Sort By → `Sorteer op`,
  Default → `Standaard`, Minimize → `Minimaliseer`, Window > Zoom → `Vergroot/verklein`** · macOS Finder Tier 1 · high.
- **eject → `Werp uit`** · Nautilus, Thunar én Double Commander `nl` („Uitwerpen”), omgezet naar de bare-stem imperatief
  · high. ❗ macOS `nl` zegt hier `Verwijder`, maar dat is in Cmdr al de vertaling van _delete_; die botsing zou een
  uitwerpactie op een verwijderactie doen lijken, dus wint hier de Tier-3-consensus van de bestandsbeheerders.
- **remove (uit een lijst, bv. een favoriet) → `Verwijder`** · macOS Finder `nl` („Verwijder uit navigatiekolom”) ·
  high. Hier is de context (het favorietenmenu) wél voldoende: er staat geen bestand op het spel.
- **ascending / descending → `Oplopend` / `Aflopend`** · Thunar + Dolphin `nl` · high.
- **changelog → `Wijzigingenlogboek`** · Microsoft-terminologie · high. Onderscheiden van Help > `Wat is er nieuw`: het
  ene noemt het document, het andere het nieuws.
- **word wrap → `Tekstterugloop`** · Microsoft-terminologie · high.
- **pin / unpin tab → `Maak tabblad vast` / `Maak tabblad los`** · Safari `nl` („Maak tabblad vast”) · high.
- **Finder-tagkleuren → `Rood, Oranje, Geel, Groen, Blauw, Paars, Grijs`** · macOS Finder (`TG_COLOR_*`) · high.
- **Tagrij (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`) → `Tags`, `Voeg ‘{color}’ toe`,
  `Verwijder ‘{color}’`** · macOS Finder (`TG5` = `Voeg '^0' toe`, `TG6` = `Verwijder '^0'`, `N169.37` = `Tags`) · high.
  Finder zet hier rechte `'`; de catalogus houdt zijn `‘…’` aan, net als `menu.context.copyNamed`.
- **busy (volume in gebruik) → `(bezet)`** · Microsoft-terminologie · high.
- **disconnect → `Verbreek verbinding`** · macOS Finder `nl` („Verbreek”), aangevuld tot een begrijpelijk label · high.
- **Bewust gelijk aan het Engels** (met `sameAsSourceJustification`): `menu.bar.help`, `menu.app.onboarding`,
  `menu.file.open`, `menu.view.zoom`, `menu.zoom.in`, `menu.zoom.percent*`, `menu.tag.rowLabel`, `menu.view.askCmdr`.

## De terugvalmelding voor de systeem-SMB-verbinding (`fileExplorer.network.osMountFallback.*`)

De melding die verschijnt als Cmdrs eigen (snellere) SMB-verbinding niet lukte en de share via macOS' eigen verbinding
draait. Toon: geruststellend, geen foutmelding. De share werkt, hij is alleen trager.

- **native (door het besturingssysteem geleverd) → `systeemeigen`** · Microsoft-terminologie (`native` → `systeemeigen`)
  · high. Gebruikt in `de systeemeigen SMB-netwerkverbinding van macOS`. Het bestaande `fileOperations.*.smbNativeNote`
  noemt dezelfde verbinding kort `de systeemverbinding`; beide slaan op één ding, de korte vorm waar de context al over
  SMB gaat, de lange waar de melding het begrip introduceert.
- **network connection → `netwerkverbinding`** · Microsoft-terminologie (`network connection` → `netwerkverbinding`) ·
  high. Samengesteld met het protocol als `SMB-netwerkverbinding` (streepje na een initiaalwoord, zoals `SMB-share`).
- **`Nx slower/faster` (snelheidsvermenigvuldiger) → `Nx zo langzaam/snel als …`** · high voor de cijfers (de
  Microsoft-stijlgids schrijft cijfers voor meeteenheden en percentages voor), tentative voor de vorm:
  `4x langzamer dan` bestaat ook, maar `4x zo langzaam als` is ondubbelzinnig (bij `langzamer dan` twist men over of de
  factor op het verschil of op het geheel slaat). Total Commander `nl` levert de bijvoeglijke vorm (`Langzamer` /
  `Sneller`, sleutels 2095/2096); niets in de stapel zet er een vermenigvuldiger voor, dus de vorm is een oordeel.
- **"Click the button below" → `Klik op de knop hieronder`** · Microsoft-stijlgids standaardiseert het voorzetsel
  (`Klik op de knop <naam>`), en de catalogus gebruikt `hieronder` al (`Klik hieronder op <restart>…`) · high.
- **"Couldn't directly connect to X" → `Direct verbinden met X lukte niet`** · de catalogusbrede weergave van "failed"
  (`X lukte niet`, zie boven) toegepast op de infinitiefgroep; vermijdt `mislukt` en `fout` volgens Cmdrs stemregel ·
  high.
- **"Try connecting directly" (knop) → `Probeer direct te verbinden`** · bare-stem imperatief zoals
  `fileExplorer.network.retry` (`Probeer opnieuw`), met het `direct` + `verbinden` van
  `fileExplorer.navigation.connectDirectly` (`Verbind direct voor snellere toegang`) en van de aanhaling
  `‘Verbind direct’` in `smbNativeNote` · high.
- **"Dismiss" (sluitknop van deze melding) → `Sluit`** · dezelfde waarde als `lowDiskSpace.toast.closeTooltip`, en het
  gevestigde `dismiss → Sluit` / MS `dismiss → sluiten` · high.
- **Cmdrs bezit → `van Cmdr`** (`de directe verbinding van Cmdr`) · de catalogus gebruikt overwegend `van Cmdr`
  (`de AI van Cmdr`, `de index van Cmdr`); de genitief `Cmdrs` komt maar één keer voor · high.

## De weigerberichten van naam wijzigen, nieuwe map en nieuw bestand (`errors.mutation.*`, `errors.volume.*`)

Eenendertig sleutels: de regel onder het naamveld (of in een korte melding) wanneer een naamwijziging of een nieuwe
map/bestand wordt geweigerd. RAW-familie, dus enkele apostroffen; `{path}` is een ongecontroleerde invoeging en staat in
de gekrulde `‘…’` van de locale-brede afspraak (het Engels gebruikt rechte `"…"`). Gemijnd in `_ignored/i18n/nl/`,
2026-08-23.

- **locked (de macOS-vlag op een onderdeel) → `beveiligd`; unlock → `de beveiliging opheffen`** · macOS Finder Tier 1:
  het aankruisvak in Toon info heet `Beveiligd` (`AXNODE1`, AppKit `AutosaveButton`), `NE17`/`PE13` zeggen "omdat het
  bestand/onderdeel '^0' beveiligd is", `NE18` "deselecteer 'Beveiligd' en probeer het opnieuw" en `BN43` "de
  beveiliging van dit onderdeel opheffen" · high. `errors.write.fileLocked.*` is meegetrokken naar `beveiligd`.
- **Get Info → `Toon info`** · macOS Finder (`NE18` "Kies 'Archief' > 'Toon info'"), al in de catalogus
  (`commands.fileGetInfo.mac.label`, `menu.file.getInfo`) · high. Staat NIET op de niet-vertalen-lijst.
- **System Integrity Protection → onvertaald** · macOS Finder `ET6` laat de naam in het Nederlands staan ("... vanwege
  System Integrity Protection") · high. Apple vertaalt deze naam niet, dus Cmdr ook niet.
- **"a volume's top folder" → `de hoofdmap van een volume`** · Xfce Thunar nl ("The root folder has no parent" → "De
  hoofdmap heeft geen bovenliggende map"), KDE Dolphin nl (`hoofdmap`); GNOME Nautilus levert het hele concept
  ("Toplevel files cannot be renamed" → "Bestanden op het hoogste niveau kunnen niet hernoemd worden") · high op
  `hoofdmap`. De zinsvorm volgt macOS Finder `RN33` ("De naam van het onderdeel '^0' kan niet worden gewijzigd"), en het
  werkwoord blijft `naam wijzigen`, nooit `hernoemen`.
- **"There's nothing at X any more" → `Er staat niets meer op ‘{path}’.`** · macOS Finder `PE131` ("'^0' bestaat niet
  meer.") geeft de Tier-1-vorm voor het verdwenen onderdeel; `staat ... op` volgt de in-catalogus
  `errors.listing.alreadyExists.explanation` ("Er bestaat al een bestand of map op `{path}`") · high. De zusterregel
  `errors.volume.alreadyExists` spiegelt hem: `Er staat al iets op ‘{path}’.`
- **"isn't available any more" → `is niet meer beschikbaar`** · macOS Finder `NE7` ("omdat de schijf '^0' niet meer
  beschikbaar is"), `NE61` ("zijn niet meer beschikbaar") · high.
- **"didn't answer in time" → `reageerde niet op tijd`** · woordelijk de in-catalogus
  `errors.listing.connectionTimedOutErrno.explanation` ("de verbinding reageerde niet op tijd"); `reageren` is de
  gevestigde rij voor `respond` (macOS AppKit) · high.
- **"no room left" → `Er is geen ruimte meer op dit volume.`** · macOS Finder `PE4` ("onvoldoende ruimte beschikbaar")
  en `PE18` ("de schijf is vol"); `geen ruimte meer` houdt de vlotte, korte toon van het Engels vast · high.
- **"lost track of" → `kwijtgeraakt`; destination folder → `doelmap`** · macOS Finder vertaalt "destination folder" met
  `doelmap` (term `destination`), en de catalogus gebruikt `doelmap` al vijf keer tegen `bestemmingsmap` één keer ·
  high. Het voornaamwoord bij `map` is `hem` ("Open hem opnieuw"), zoals `Cmdr maakt hem aan`.
- **"the archive edit" → `de archiefbewerking`** · samenstelling van het gevestigde `archief` + `bewerking` (macOS
  `bewerking` voor een bestandsbewerking); sluit aan op `Bewerkingenlogboek` en op de wachtrijregel "Bezig met archief
  bewerken" · high.
- **"Something went wrong" → `Er ging iets mis`** · al in de catalogus (`onboarding.cloudSetup.status.genericError`) ·
  high. "mis" staat hier in lopende tekst, niet als label, dus de stemregel tegen `fout`/`mislukt` blijft overeind.
- **"start up" (de app) → `opstarten`** · Microsoft-terminologie (29 treffers `opstarten`), en de catalogus heeft
  `opstartschijf` al · high.
- **"on its way out" (verwijdering in behandeling) → `op weg naar buiten`** · woordelijk de in-catalogus
  `errors.listing.deletePending.explanation` · high.
- **"restarted its connection" (MTP-sessie) → `heeft zijn verbinding opnieuw gestart`** · woordelijk de in-catalogus
  `errors.listing.deviceReconnecting.explanation` ("De verbinding met het apparaat ... is opnieuw gestart. Het apparaat
  is nog steeds aangesloten") · high. Het apparaat is NIET losgekoppeld: `losgekoppeld` is voorbehouden aan
  `deviceDisconnected`.
- **"the change may still land" (time-out, geen weigering) → `de wijziging gaat misschien toch nog door`** · de staart
  volgt `fileOperations.mkdir.timeoutMessage` ("dus de map is misschien toch aangemaakt") en
  `fileExplorer.rename.unconfirmed` ("dus de naam is misschien toch gewijzigd") · high. Nergens een woord dat de
  bewerking als afgelopen of geweigerd voorstelt.
- **"Pick a different one" → `Kies een andere`** · `naam` is een de-woord, dus `een andere` zonder `-e`-val; de eerste
  zin hergebruikt `errors.listing.invalidName.explanation` ("een naam die de bestemming niet kan opslaan") · high.
- **"at your request" → `op je verzoek`** · onbeklemtoond `je` volgens `style.md` (geen contrast met iemand anders);
  `stoppen` volgt de macOS-rij `quit → Stop` · high.

## De twee prullenmandweigeringen (`errors.mutation.trash*`)

Twee sleutels die na de pass hierboven zijn toegevoegd, met dezelfde vorm: één platte regel onder het naamveld of in een
korte melding, RAW-familie, enkele apostroffen. Gemijnd in `_ignored/i18n/nl/`, 2026-08-23.

- **"This volume has no Trash" → `Dit volume heeft geen prullenmand`** · `prullenmand` is de gevestigde rij (macOS
  Finder Tier 1) en `volume` de gevestigde rij; `dit volume` sluit aan op de zusterregel `errors.volume.storageFull`
  ("Er is geen ruimte meer op dit volume.") · high. Bewust anders dan het oudere
  `errors.write.trashNotSupported.message` ("Dit volume ondersteunt de prullenmand niet."), want het Engels zegt hier
  ook `has no`, niet `doesn't support`.
- **"the only way is to delete permanently" → `dus definitief verwijderen is de enige manier`** ·
  `definitief verwijderen` is de gevestigde rij (macOS "definitief"), woordelijk gelijk aan de knop
  `fileExplorer.functionKeyBar.deletePermanentlyAction` waar de zin de gebruiker naartoe stuurt · high op de term,
  compositioneel op `de enige manier` (de stapel heeft geen zin met "the only way"). Het genominaliseerde infinitief als
  onderwerp vermijdt een lijdend voorwerp, en dus een voornaamwoord dat het geslacht van het onderdeel zou moeten raden
  (`het bestand` vs `de map`).
- **"macOS wouldn't move this to the Trash." → `macOS wilde dit niet naar de prullenmand verplaatsen.`** · de
  woordvolgorde volgt macOS AppKit ("The file could not be moved to the trash." → "Het bestand '%@' kon niet naar de
  prullenmand worden verplaatst") en Finder `MT2_V1` ("kan niet naar de prullenmand worden verplaatst"), maar in de
  bedrijvende vorm met `macOS` als onderwerp, zoals `errors.listing.notPermitted.explanation` ("macOS blokkeerde de
  toegang van Cmdr tot ...") · high. `wilde ... niet` is de gewone Nederlandse weergave van het Engelse `wouldn't` en
  houdt de kalme toon vast; het kale `dit` (geen `dit bestand` / `deze map`) blijft neutraal over wat er geweigerd is.

## De drie crashdialoog-openingen (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`)

Het dialoogvenster bij de volgende start kiest nu één van drie zinnen, afhankelijk van wat het rapport heeft vastgelegd.
`.ended` blijft ongewijzigd; de twee nieuwe **mogen niet zeggen dat Cmdr is gecrasht, gestopt of afgesloten**, want dat
is precies wat er niet gebeurde. Alle drie delen dezelfde opening `Cmdr is de vorige keer …`, overgenomen uit de
onaangeroerde `.ended`.

- **"ran into a problem" → `een probleem tegengekomen`** · `tentative`. macOS Finder `nl` `NE105` ("'^0' heeft een fout
  aangetroffen") levert de vorm met de app als onderwerp, maar `aangetroffen` is stijf voor Cmdrs toon; de Microsoft
  `nl` stijlgids (p. 48/52) schrijft juist de onpersoonlijke vorm voor ("Er is een probleem opgetreden bij …"), die Cmdr
  niet als onderwerp kan nemen zoals het Engels doet. `tegenkomen` is gekozen op de macOS-vorm en draagt geen enkele
  betekenis van "beëindigd".
- **"kept running" → `is gewoon blijven werken`** · `tentative`. Dubbele infinitief in de voltooide tijd, passend bij de
  zustersleutel `.ended`; `gewoon` draagt de geruststelling van het Engelse "kept". Bronnen voor het semantische veld:
  Dolphin `nl` ("is nog steeds actief"), macOS Finder ("terwijl de Finder actief is"), Thunar ("draaiende … instantie").
  ❌ Niet `doorlopen`: dit catalogusveld reserveert dat voor een _bewerking_ die doorloopt (`queueTooltip`,
  `stallUnknown`, `backgroundedToast`), niet voor de app zelf. ❌ Niet `bleef gewoon draaien`: korter en spreektaliger,
  maar de catalogusregel uit de hernoemingsronde wil de voltooide tijd voor gebeurtenissen in het recente verleden.
- **"in the background" → `op de achtergrond`** · de al vastgelegde regel, uit Double Commander `nl` ("Werk op de
  achtergrond") · high. Echt afwezig in macOS/Nautilus `nl`: die kennen alleen de visuele achtergrond
  (bureaubladafbeelding), geen achtergrond*taak*.
- **"a report" (zonder "crash") → `een rapport`** · de vastgelegde regel `report → rapport` (Microsoft `nl` stijlgids p.
  54), naast het cataloguswoord `crashrapport`. De tweede zin is die van `.ended`, met `crashrapport` vervangen door
  `rapport` · high.
- **`.unknown` zegt niets over de afloop**: alleen dat er een probleem was. Daardoor klopt de zin of Cmdr nu is gestopt
  of gewoon is doorgegaan.
- Geen van beide waarden bevat een apostrof, dus de ICU-verdubbelingsregel speelt hier niet.

## De instellingstekst voor rapporten dekt nu beide gevallen (`settings.updates.crashReports.description`)

De schakelaar stuurt ook een rapport wanneer een probleem op de achtergrond de app NIET heeft gestopt, dus de hulptekst
mag niet langer alleen over stoppen gaan. Alles komt uit de crashdialoog-sectie hierboven, in de tegenwoordige tijd:

- **`wanneer Cmdr onverwachts stopt`** uit `crashReporter.dialog.body.ended`;
  **`op de achtergrond een probleem tegenkomt`** uit `.keptRunning` · `tentative` voor `tegenkomen` (om de reden die
  daar staat), `high` voor de rest.
- **`een rapport`** zonder `crash`, omdat de zin beide gevallen dekt · high. ❌ Het LABEL
  `settings.updates.crashReports.label` blijft `Crashrapporten sturen`: dat is de naam van de instelling.
- **Tweede zin uit `crashReporter.dialog.privacyNote`** (`welk deel van de code het probleem tegenkwam`), in plaats van
  `crashlocatie`, dat alleen klopte bij een crash · high. Meteen ook `app-versie` → `appversie`: gesloten samenstelling,
  zoals privacyNote al schreef, zodat één veld op twee schermen niet twee spellingen krijgt.

## Get Info en Beveiligd (macOS-UI-namen die Apple wél vertaalt)

Twee waarden lieten de Engelse macOS-namen staan in verder Nederlandse zin (`errors.write.fileLocked.suggestion.mac`,
`errors.write.permissionDenied.suggestion.deleteMac`). Dat is rechtgezet: Apple vertaalt beide namen, dus
term-keuzeprincipe 1 ("localize what Apple localizes") geldt hier.

- **"Get Info" → `Toon info`** · macOS Finder `nl` (`Localizable.json`: `"Get Info"` = `"Toon info"`; en
  `"Shows the Get Info window for an item or items"` = `"Toont het venster 'Toon info' voor een of meer onderdelen"`) ·
  high. In de catalogus al de vaste vorm: `menu.file.getInfo`, `commands.fileGetInfo.mac.label` en
  `errors.mutation.fileLocked` schrijven alle drie `Toon info`. De twee gerepareerde waarden waren de laatste
  achterblijvers.
- **"Locked" (het aankruisvak in het infovenster) → `Beveiligd`** · macOS Finder `nl` (`InfoWindowGeneralView.json`
  `1073.title` = `Beveiligd`; `LocalizableMerged.json` `AXNODE1` = `Beveiligd`) en AppKit `nl` (`AutosaveButton.json`
  `"Locked"` = `"Beveiligd"`) · high. ❌ Niet `Vergrendeld`: AppKit reserveert `vergrendeld` voor het document-slot in
  de prullenmand, en het aankruisvak dat de gebruiker in Finder ziet heet `Beveiligd`.
- **"uncheck Locked" → `deselecteer ‘Beveiligd’`** · Apple's eigen zin voor precies dit herstel-advies, macOS Finder
  `nl` `LocalizableMerged.json` `NE18`:
  `"Kies 'Archief' > 'Toon info', deselecteer 'Beveiligd' en probeer het opnieuw."` (Engelse zijde:
  `"Choose File > Get Info, deselect “Locked,” and then try again."`). Ook `BN43` gebruikt
  `deselecteer het aankruisvak 'Beveiligd'` · high. Aanhalingstekens volgens de stijlregel `‘…’`, zoals
  `errors.mutation.fileLocked` al doet.
- **Eén woord voor de bestandsstaat: `beveiligd`.** `errors.write.fileLocked.title`/`.message` en
  `errors.write.permissionDenied.suggestion.deleteMac` zeggen het nu ook, zoals `errors.mutation.fileLocked` en Apple.
  `vergrendeld` blijft alleen voor een ander slot (de index-lock van Git, `errors.git.indexLocked.message`).

## De negen uitwerp- en verbreekzinnen (`errors.eject.*`)

Elke waarde valt achter een dubbele punt in `fileExplorer.pane.ejectFailedToast`
(`{volumeName} uitwerpen lukte niet: …`) of `fileExplorer.pane.disconnectFailedToast`
(`Verbinding verbreken lukte niet: …`), dus de zin moet als vervolg lezen en mag het wikkelwerkwoord niet herhalen.
`errors.*` is een RAW-familie: gewone apostrofs, geen ICU. Geen van de negen waarden bevat er een.

- **eject → `uitwerpen` / `Werp … uit`** · de al vastgelegde regel (Nautilus, Dolphin, Microsoft; macOS' eigen
  `Verwijder media` botst met _delete_) · high. Voltooid deelwoord `uitgeworpen`; de catalogus had het al in
  `errors.listing.remotePermissionDenied.suggestion` ("werp de share dan uit").
- **drive → `schijf`, een de-woord** · termregel `drive/disk → schijf` · high. Voornaamwoord dus `hij` / `hem` ("Werp
  hem uit", "hij blijft aangesloten"), net als `onthoudt of hij aan of uit staat`.
- ❌ **"removable" NIET als `verwijderbaar`**, hoewel macOS `nl` dat gebruikt (`"Removable"` = `"Verwijderbaar"`,
  `"Removable Volume"` = `"Verwijderbaar volume"`). In een zin over uitwerpen leest `verwijderbaar` als _te
  verwijderen/te wissen_, precies de botsing waarom `Verwijder` als vertaling van _eject_ al was afgewezen.
  `errors.eject.notEjectable` zegt daarom wat de gebruiker kan doen: `Deze schijf kun je niet uitwerpen` · high.
- **"is in use" → `is in gebruik`** · macOS Finder `nl` ("Het volume kan niet worden verwijderd, omdat het momenteel in
  gebruik is", "'^0' is in gebruik en kan niet worden verwijderd") · high.
- **"Close any open files and apps" → `Sluit open bestanden, stop geopende apps`** · macOS Finder `nl` ("Sommige
  bestanden op deze schijven zijn mogelijk in gebruik. Stop geopende apps en probeer het opnieuw.") levert
  `stop geopende apps` letterlijk · high.
- **"wouldn't …" → `wilde … niet …`** · de cataloguslijn uit `errors.mutation.trashRefused` ("macOS wilde dit niet naar
  de prullenmand verplaatsen") · high. Vandaar `Het apparaat wilde de verbinding niet verbreken`.
- **"unplug it" → `koppel het los`** · catalogus `mtp.permissionDialog.helpText` ("Koppel het apparaat … los") en
  `errors.listing.deviceProblem.suggestion` ("Koppel het apparaat los en sluit het opnieuw aan") · high.
- **"once it's idle" → `zodra het niet meer bezig is`** · tegenhanger van `fileExplorer.mtp.deviceBusy` ("Het apparaat
  is bezig") · high.
- **"hasn't answered yet" → `heeft nog niet gereageerd`** · exact de zusterzin `errors.mutation.timedOut` ("Het volume
  heeft nog niet gereageerd, dus de wijziging gaat misschien toch nog door"), zelfde vorm, zelfde geruststelling · high.
- **"network share" → `netwerkshare`** · de vastgelegde regel, en de vorm die `errors.json` elders al gebruikt
  (`errors.listing.remotePermissionDenied.explanation`) · high. `gedeelde map` is gereserveerd voor de SMB-map-lijsten
  in `fileExplorer.json`.
- **"Something went wrong, and Cmdr couldn't tell what." → letterlijk dezelfde Nederlandse zin als
  `errors.mutation.unexpected`** ("Er ging iets mis, en Cmdr kon niet achterhalen wat.") · high. Zelfde Engelse bron,
  zelfde toast-rol; één vertaling voor beide houdt de vangnetzin herkenbaar.
- **`errors.eject.notAnSmbVolume` herhaalt `verbreken` bewust** na de wikkel `Verbinding verbreken lukte niet:`
  (`Dit is geen netwerkshare, dus er is geen verbinding om te verbreken.`): het Engels herhaalt net zo, en `verbreken`
  heeft een object nodig.

## De twee knoppen van de prullenmandmelding en de terugzet-familie (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

Negen nieuwe sleutels: de twee knoppen op de melding die verschijnt nadat bestanden naar de prullenmand zijn verplaatst,
de voortgangs- en resultaatteksten van het terugzetten, en de opdracht "Go to trash" in het opdrachtenpalet.

- **"Undo" (de knop op deze melding) → `Zet terug`** · macOS Finder `N153.1` Tier 1: `Put Back` = `Zet terug`, precies
  deze handeling (onderdelen uit de prullenmand terug op hun oude plek) · high. Twee woorden, negen tekens, past in een
  smalle melding.
  - ❌ NIET `Herstel` (Finder `ME13`, het Wijzig-menu): buiten dat menu is `Herstel` dubbelzinnig, want macOS gebruikt
    hetzelfde woord voor `Revert` (`AppKit/Document`) en voor `Repair` (`N175` = "Herstel alias…"). Op een melding vlak
    na een verwijderactie leest dat als "repareer".
  - ❌ NIET `Ongedaan maken` (wat `askCmdr.renameUndo.undo` wel gebruikt): veertien tekens is te lang naast de tweede
    knop. De afwijking is verdedigbaar omdat Finder `Zet terug` uitsluitend voor de prullenmand gebruikt; de generieke
    undo-knop elders blijft `Ongedaan maken`.
- **put back (het werkwoord) → `terugzetten` / `teruggezet`** · macOS Finder `PE130_V2` ("could not be put back" =
  "konden niet worden teruggezet") + de al verzonden `askCmdr.renameUndo.undone`/`.undoing` · high. Eén stam voor knop,
  voortgang en resultaat.
- **"Go to trash" → `Ga naar prullenmand`** · werkwoord uit Finder `TL_HELP_TCAN` ("Go to the Trash" = "Ga naar de
  prullenmand"), lidwoord weggelaten volgens Finders eigen knopvorm `N83` ("Go to Folder…" = "Ga naar map…") · high.
  Dezelfde waarde op de knop en op het opdrachtenpalet-label, net als in het Engels.
  - ❌ NIET `Naar prullenmand`: die waarde betekent in dit bestand al _verplaatsen_ naar de prullenmand
    (`delete.trashSwitch`), dus op deze melding zou hij als "nog een keer weggooien" lezen.
- **"Putting them back..." → `Bezig met terugzetten...`** · dezelfde vorm als de zustervoortgang in dit bestand
  (`transferDialog.checkingConflicts` = "Bezig met controleren op conflicten..."). Het `nl`-bestand houdt hier drie
  punten aan, zoals het Engelse origineel, niet `…`.
- **De tweede helft congrueert gewoon.** Bij `{skippedText}` in `undonePartial` hoort het gehele getal `{skipped}`, dus
  het werkwoord kan de takken in:
  `… teruggezet; {skippedText} {skipped, plural, one {onderdeel bleef} other {onderdelen bleven}} in de prullenmand.`
  Het getelde woord is `onderdeel`, het gevestigde woord voor het `item` dat de bron in deze helft zegt, niet `bestand`
  zoals in de eerste helft. Komt er ooit een los `*Text`-getal ZONDER plural-partner langs, dan is de weglaat-truc de
  uitweg: houd die helft werkwoordloos, parallel aan een deelwoord ernaast.
- **"Nothing to put back. …" →
  `Er is niets terug te zetten. Deze onderdelen staan misschien al terug, of hun schijf is niet verbonden.`** · zet de
  zin van de zuster `askCmdr.renameUndo.unavailable` voort ("Er is niets terug te zetten. Deze groep is misschien al
  ongedaan gemaakt, of de schijf is niet verbonden.") · high. `onderdeel` is het gevestigde woord voor "item"
  (`delete.overflowMore`), `schijf` voor "drive".
- **"This drive doesn't keep a trash." → `Deze schijf heeft geen prullenmand.`** · de al vastgelegde vorm
  `Dit volume heeft geen prullenmand`, met `schijf` omdat het Engels hier `drive` zegt · high. Een feitelijke
  mededeling, dus niet het register van `errors.write.trashNotSupported.message` ("ondersteunt … niet").
- **De opdrachtbeschrijving → `Open de prullenmand van de schijf die je bekijkt`** · imperatief, zoals de andere
  beschrijvingen in `commands.json` ("Maak een kopie van de geselecteerde bestanden in dezelfde map"), en `je` volgens
  de vastgelegde aanspreekvorm · high.

## Aanvullen wat al verstuurd is: het notitievenster bij een automatisch foutrapport (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

Elf sleutels: Cmdr heeft een foutrapport uit zichzelf verstuurd (de gebruiker koos daarvoor), de melding zegt dat, en
een knop op die melding opent een venster met precies wat er verstuurd is plus een veld voor een notitie die AAN
DATZELFDE rapport wordt gehangen. Er gaat niets een tweede keer omhoog. Gemijnd in `_ignored/i18n/nl/`, 2026-08-28.

- **"Add to X" (knop) → `Voeg aan X toe`** · macOS AppKit Tier 1: `Voeg aan begin van knoppenbalk toe`,
  `Voeg de liniaal aan de stijl toe`, `Voeg het lettertype aan de stijl toe` · high. Het scheidbare `toe` staat
  achteraan, dus `Add to report` wordt `Voeg aan rapport toe`, niet het Engels-geordende "Voeg toe aan rapport". De
  bare-stem imperatief volgt de knopregel in `style.md`.
- **add (aan iets bestaands) → `toevoegen` / `toegevoegd`** · macOS Finder ("Als je personen aan dit document wilt
  toevoegen …", "De server '^0' kan niet aan je favorieten worden toegevoegd") · high. Eén stam voor titel, knop,
  voortgang en bevestiging.
- **Venstertitel = infinitiefgroep, niet de imperatief.** `Add to your error report` → `Toevoegen aan je foutrapport`,
  parallel aan de zustertitel `errorReporter.dialog.title` (`Send error report` → `Foutrapport versturen`) · high. De
  imperatief is voorbehouden aan knoppen.
- **"Adding…" → `Toevoegen…`** · zelfde vorm als de zuster `errorReporter.dialog.sending` (`Sending…` → `Versturen…`) ·
  high. Eén teken `…` (U+2026), zoals het Engels.
- **note → `notitie`** (niet `bericht`) · MS `note` → `notitie`, en dit bestand heeft het al vast in
  `dialog.noteLabel`/`noteTooLong` · high. De rij "note (user's message) → bericht" hierboven komt uit de feedback-pass;
  binnen `errorReporter.json` wint `notitie`, anders krijg je twee woorden voor hetzelfde veld.
- **"What was sent" → `Wat er verstuurd is`** · de voltooide tegenhanger van het bestaande `dialog.detailsToggle`
  (`What''s about to be sent` → `Wat er verstuurd gaat worden`) · high. Zelfde zinsbouw, alleen de tijd verschilt, zoals
  in het Engels.
- **"the Help menu" → `het Help-menu`** · de menubalk heet in het Nederlandse macOS `Help` (Finder `nl`, zie § Native
  menu's), dus geen vertaling van "Help" · high. Streepje na een Engelse eigennaam, net als `SMB-share`. De bijbehorende
  opdracht heet in de catalogus `Verstuur foutrapport…` (`menu.help.sendErrorReport`); het Engels noemt alleen het menu,
  dus die zin doet dat ook.
- **"can't take a note any more" → `Aan dat rapport kun je niets meer toevoegen`** · high. Geen `fout` en geen
  `mislukt`: dit is een mededeling over wat er nog kan, niet over wat er misging (de stemregel in `style.md`).
- **"To get your notes to the team, …" → `Om je notities alsnog bij het team te krijgen, verstuur je …`** · de
  doelzin-met-`je` van Finder ("Als je personen aan dit document wilt toevoegen, verplaats je het naar iCloud") · high.
  `alsnog` vangt het Engelse "instead" zonder een tweede `je je` op te leveren.
- **"Couldn't add your note: {error}" → `Je notitie toevoegen lukte niet: {error}`** · de catalogusbrede weergave van
  "couldn't X" (`X lukte niet`, zie `dialog.saveFailedToast` = `Bundel bewaren lukte niet`) · high.
- **"Note added to your report." → `Notitie toegevoegd aan je rapport.`** · deelwoord vooraan, zoals de zustermelding
  `sentToast.message` (`Foutrapport verstuurd. Je referentie-ID is`) · high. De melding eindigt vlak vóór de badge, dus
  zonder leesteken na `is`.
- **"View or add notes to the report" (knop op de melding) → `Bekijk het rapport of voeg notities toe`** · high voor de
  woorden, tentative voor de lengte. Beide helften moeten blijven staan (kijken én toevoegen), en het gedeelde lijdend
  voorwerp kan in het Nederlands niet vóór beide werkwoorden staan, dus het rapport wordt bij het eerste werkwoord
  genoemd en `notities` bij het tweede. De kortere `Bekijk of vul het rapport aan` viel af: `aanvullen` benoemt de
  notitie niet, terwijl het venster er precies om draait.

## Het selectievenster: bestanden selecteren en deselecteren (`selection.*`)

- **select (bestanden via een patroon) → `Selecteer`; deselect → `Deselecteer`** · macOS Finder `nl`, `MenuBar.json`
  `172.title` = „Selecteer alles” (Tier 1, nagekeken op het draaiende systeem, macOS 26.6.2, build 25G83, 2026-08-29).
  Finders tegenhanger van „Deselect All” (`MenuBar.json` `300488.title`) is „Maak selectie ongedaan”: dat is de hele
  selectie, en er past geen lijdend voorwerp in. Het overgankelijke werkwoord staat wél bij Apple, in Finder
  `LocalizableMerged` `NE18`: „Selecteer het, kies 'Archief' > 'Toon info' en deselecteer het aankruisvak 'Beveiligd'.”
  Double Commander `nl` (de orthodoxe tweepaneelmanager uit Cmdrs lijn) doet hetzelfde: „Unselect a Gro&up...” →
  „Deselecteer een groep...”, „&Unselect All” → „Deselecteer alles” · `high`. Microsofts „selectie opheffen” (term-id
  `44742`, NLD/BEL) is de Windows-vorm en verliest van macOS.
- **De catalogus wijkt hier bewust van Finders „Maak selectie ongedaan” af.** `menu.select.deselectAll` gebruikt
  `Deselecteer alles` (Double Commander, en het rijmt op `Selecteer alles`), terwijl `menu.select.deselectFiles`,
  `commands.selectionDeselectFiles.label` en `selection.dialog.title.remove` allemaal `Deselecteer bestanden` zeggen. De
  dialoogtitel moet gelijk blijven aan het menu-item dat hem opent; dat is precies wat dit katalogusdeel repareert.
- **recent selections → `recente selecties`** · gespiegeld aan de zusters in `queryUi.recent.*` („recente
  zoekopdrachten”), zelfde zinsbouw, alleen `zoekopdrachten` → `selecties`. `selectie` is Finders eigen woord („Nieuwe
  map met selectie”, „Open selectie”) · `high`.
- **`selection.recent.applyAria` volgt `search.recent.runAria`** · daar staat „Voer recente {mode}-zoekopdracht uit:
  {query}”, hier „Pas recente {mode}-selectie toe: {query}”. `apply` → `Pas toe` uit macOS AppKit (`NSFontOptionsPanel`
  `100411.title` en `NSPreferences` `7TY-1Z-cs2.title` = „Pas toe”) · `high`. Het scheidbare partikel `toe` staat
  achteraan, zoals `style.md` voorschrijft; `{query}` staat helemaal achteraan na de dubbele punt, zodat elke
  gebruikerstekst past.
- **De Enter-toets heet in het Nederlands `Enter`** · `search.runHint` zegt al „Druk op Enter om te zoeken”, dus „Druk
  op Enter om te filteren” · `high`. Niet `Return`, niet `Invoer`.
- **De tooltip is een eigen zin en hoeft de knoptekst niet te herhalen.** `QueryDialog.svelte` bouwt de toegankelijke
  naam van de knop uit `config.primaryAction.ariaLabel ?? config.primaryAction.label`, dus uit de label-sleutel; de
  tooltip hangt via `use:tooltip` aan een binnenste `span`. WCAG 2.5.3 klopt daarmee al door de opbouw, en de catalogus
  laat de twee elders bewust uiteenlopen (`search.action.showAll.label` tegenover zijn `.tooltip`). Hier loopt het
  Nederlands toevallig wél door („Selecteer deze bestanden in het actieve paneel”), en dat is prima, maar het is geen
  eis. `actieve paneel` komt uit `commands.navGoToPath.description` en `commands.favoritesAdd.description` · `high`.
- **`selection.notice.snapshotPane` → „Er wordt vergeleken met wat in de lijst staat (het volledige pad).”** ·
  geruststellend, geen waarschuwing, zoals het `@key` vraagt; `het volledige pad` is de catalogusvorm (zie
  `errors.listing.nameTooLongErrno.explanation`) · `high`.

## Eén ding, één naam: de interne driftronde

`desktop-i18n-term-consistency` vond 38 divergenties in `nl` (één Engelse waarde, twee of meer Nederlandse vormen) — het
hoogste aantal van alle locales. Eenentwintig waren echte drift, zeventien zijn bewuste grenzen. De drift kwam bijna
altijd doordat een latere ronde een woord opnieuw besliste en alleen de eigen bestanden bijwerkte. De regel die de
meeste gevallen beslecht stond al in `style.md`: **knoppen en menu-items krijgen de kale gebiedende wijs van macOS**.

### De gerepareerde drift

- **send → `Verstuur` (knop) / `X versturen` (venstertitel)** · macOS Finder (`Send` → `Verstuur`) · high. `Stuur` is
  weg. `Send error report` had vijf vindplaatsen en drie vormen; `Send feedback` en `Send report` splitsten hetzelfde.
- **`queue.row.dismiss` → `Sluit`** (was `Wis`), plus `dismissAria` en `queue.toolbar.dismissAll`. ❗ `Wis` betekent
  "uitvegen", terwijl de `@key` juist zegt dat er níets ongedaan wordt gemaakt, opnieuw geprobeerd of verwijderd. Dit is
  precies de klasse fout die in `zh-Hant` een Dismiss-knop als _Negeren_ liet lezen.
- **Delete permanently → `Verwijder definitief`** · macOS Finder `nl` gebruikt `definitief verwijderen` in lopende tekst
  en `Verwijder onmiddellijk…` als menu-item (dus werkwoord eerst) · high. Drie sleutels, één vorm; `permanent` is weg.
- **disconnect (netwerk) → `Verbreek`** · macOS Finder `nl` (`MR10.1`, `N200`, AppKit `SavePanel`) · high. De `@key`
  vraagt letterlijk om Finders eigen woord, en dat is de kale vorm.
- **alleen-lezen samenstellingen sluiten**: `alleen-lezenvolume`, `alleen-lezenapparaat` · macOS `nl` ("bevindt zich op
  een alleen-lezenvolume") · high.
- **Preview → `Voorvertoning`** · macOS AppKit + Finder `TL45` · high. `Voorbeeld` is weg.
- **Got it → `Begrepen`** · macOS · high (stond al in deze lijst; `Duidelijk` was de uitschieter).
- **operation log → `Bewerkingenlogboek`** · sluit aan op `Bewerkingenwachtrij` uit de wachtrijronde · high.
- **device → `apparaat`** (niet `toestel`), **permission (bestandsrechten) → `bevoegdheid`** (niet `toestemming`) ·
  macOS `nl` ("leesbevoegdheden", "bevoegdhedenfout"); `toestemming` houdt Apple voor toestemming-tot-delen · high.
- **Stop → `Stop`** overal · macOS Finder `nl` (`BN62`, `PE107`, `SD23`) · high. De twee sleutels die daardoor gelijk
  aan het Engels werden, dragen nu dezelfde `sameAsSourceJustification` als `askCmdr.composer.stop`.
- **`Voeg aan favorieten toe`** in alle drie de sleutels: `style.md` § Formality mechanics eist het scheidbare partikel
  aan het EINDE, en `Voeg toe aan favorieten` overtrad dat.
- Verder op één vorm gebracht: `Try again` → `Probeer opnieuw`, `Check for updates` → `Zoek naar updates`,
  `Reset (all) to default(s)` → `Herstel …`, `Create new file` → `Maak nieuw bestand`, `Go to home folder` →
  `Ga naar thuismap`, `Searching…` → `Zoeken…`, `Deleting…` → `Verwijderen…`, `This volume doesn't support trash` →
  `Dit volume ondersteunt de prullenmand niet`, en de twee bèta-aanmeldzinnen.

### De grenzen die je NIET moet gladstrijken

- **Gebiedende wijs vs. infinitief is een echte grens**, geen slordigheid:
  - **Knop / menu-item → kale gebiedende wijs**: `Verstuur foutrapport`, `Toon verborgen bestanden`,
    `Herstel naar standaard`, `Verwijder definitief`.
  - **Venstertitel → `X versturen`**: `Foutrapport versturen`, `Feedback versturen`, `Rapport versturen?`.
  - **Instellingslabel (aankruisvakje / veld) → infinitief achteraan**: `Verborgen bestanden tonen`,
    `Bestandsextensies in de naamkolom tonen`. Dit is de huisvorm van de hele `settings.*`-familie.
- **Menubalktitels zijn Apple's woorden, ook als ze raar lijken**: `Archief` (File), `Wijzig` (Edit), `Weergave` (View),
  `Vergroot/verklein` (Window > Zoom). Naast elke titel staat het gewone woord voor hetzelfde begrip — `Bestand` voor
  een bestand, `Bewerk` voor het bewerken van een bestand (F4), `Toon` voor het tonen van een bestand (F3), `Zoom` voor
  tekstzoom. Beide zijn goed. ❌ Vervang een menubalktitel nooit door het gewone woord.
- **Undo heeft drie correcte vormen, want het zijn drie handelingen**: `Herstel` = macOS' ⌘Z-tekstherstel
  (`menu.edit.undo`, Tier 1, moet letterlijk kloppen); `Zet terug` = Finders "Put Back", en
  `fileOperations.trash.undoAction` dóét letterlijk een put-back; `Ongedaan maken` = het ongedaan maken van een Ask
  Cmdr-hernoemronde, en dat is het werkwoord van de hele `askCmdr.renameUndo.*`-familie (`undoLabel`, `undoJob`,
  `refusedBatches`, `unavailable`).
- **`Vorige`/`Volgende` vs. `Terug`**: `Vorige` hoort bij een gekoppeld vorige/volgende-paar (het menu Ga, de
  onboarding-wizard); `Terug` is de losse terugknop in een lijst waar je in afdaalt (netwerkbrowser). macOS `nl` levert
  `Vorige` voor het paar en `Ga terug` voor de navigatieopdracht, die `commands.navBack.label` overneemt.
- **Datums vs. deelwoorden**: kolomkoppen dragen Finders `Aanmaakdatum` / `Bewerkingsdatum`; de datumballon draagt de
  deelwoordfamilie `Aangemaakt` / `Laatst geopend` / `Laatst verplaatst` / `Laatst gewijzigd`.
- **`Gewijzigd` ≠ `Bewerkingsdatum`**: `shortcuts.section.filterModified` filtert op sneltoetsen die de gebruiker heeft
  veranderd, niet op de wijzigingsdatum van een bestand.
- **`Actief` (server draait) vs. `Bezig` (taak loopt)** voor `Running`, en **`Gereed` (levenscyclusstatus) vs. `Klaar`
  (wat een schermlezer na een afgevinkte stap voorleest)** voor `Done`. Beide splitsingen noemt de check zelf als
  voorbeelden van terechte splitsingen.
- **`Selecteer` (bestanden) vs. `Kies` (een optie uit een keuzelijst)**: `ui.select.placeholder` en
  `onboarding.stepAi.cloud.pickerTitle` kiezen, ze selecteren geen bestanden.
- **`Zoek` (de knop die de zoekopdracht start) vs. `Zoeken` (de naam van de functie)**: venstertitel, instellingssectie
  en de rij in de onboarding-tabel zijn zelfstandige naamwoorden.
- **`Van` / `Naar`** zijn een gekoppeld kopjespaar in het overdrachtsvenster; `Vanuit:` is het inline label vóór een
  pad.
- **`Bezig met …`** blijft de vorm voor een lopende bewerking in de wachtrijlijst (`queue.row.label`); een korte inline
  voortgangsaanduiding mét beletselteken gebruikt het kale `Zoeken…` / `Verwijderen…`.

### Wat deze ronde bewust NIET heeft aangeraakt

`bewaren` en `opslaan` staan allebei in de catalogus (24 resp. 31 waarden). Dat is grotendeels een terecht verschil —
macOS' knop is `Bewaar`, terwijl `opgeslagen` het gangbare bijvoeglijke deelwoord is ("Opgeslagen wachtwoord") — maar
niet overal. Een aparte ronde moet dit uitzoeken; het is te groot om er in een driftronde half doorheen te lopen.

## Wat het Engels over zichzelf rechtzette, en wat dat voor `nl` betekende

Het `en`-bestand haalde vijf tegenstrijdigheden uit zichzelf. Dit is wat daarvan in het Nederlands is beslist.

### Het voorbeeldmailadres: `jij@example.com`

- **Lokaal deel vertaald, domein blijft `example.com`** · MS-terminologie nl (`iemand@example.com`, `user@example.com`),
  RFC 2606 · high. Alle drie de velden dragen dezelfde waarde: `settings.updates.emailPlaceholder`,
  `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`.
- `jij@` volgt het `je`/`jij`-register uit `style.md`; het is de directe tegenhanger van het Engelse `you@`. De
  Microsoft-terminologie vertaalt het lokale deel ook (`iemand@`), dus dat is de gangbare praktijk.
- ❌ NIET `voorbeeld.com`: dat is een echt registreerbaar domein en kan dus iemands echte adres zijn. `example.com` is
  het domein dat RFC 2606 voor voorbeelden reserveert.

### De naam-terugzetzin noemt nu haar object

- `askCmdr.renameUndo.undone` / `.partial` →
  **`{count, plural, one {De oude naam van {countText} bestand is teruggezet} other {De oude namen van {countText} bestanden zijn teruggezet}}.`**
  · zelfde stam als de zusters (`.undoing` = "De oude namen worden teruggezet…", `.skipReason.failed.*` = "kon de oude
  naam niet terugzetten") · high.
- Het Engels gaf de naam-undo en de prullenmand-undo één zin ("Put back {countText} {files}."); dat is nu gesplitst
  doordat het Engels het OBJECT noemt. `nl` houdt de vastgelegde stam `terugzetten` / `teruggezet` voor allebei, want
  Finder `PE130_V2` levert die stam, en de dubbelzinnigheid verdwijnt zodra `de oude naam` in de zin staat.
- Nederlands congrueert hier op twee plekken tegelijk (`naam`/`namen` én `is`/`zijn`), dus de hele zin gaat de
  plural-takken in en `{countText}` staat binnenin, precies zoals bij `fileOperations.trash.undonePartial`.
- `fileOperations.trash.undone` blijft ongewijzigd.

### macOS-paneelnamen in de foutteksten

Acht `errors.*`-sleutels droegen Engelse paneelnamen. Die zijn nu runtime-tokens of Nederlandse Apple-woorden.

- `{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` blijven **letterlijk staan**: de app vult ze met
  de paneelnaam zoals die op de Mac van de GEBRUIKER heet.
- De paneelnamen die géén token hebben, gaan in Apple's eigen Nederlands:
  - **Apple Account → `Apple Account`** (onvertaald!) · macOS 26.6.2 (25G83),
    `AppleIDSettings.appex/Contents/Resources/InfoPlist.loctable` `nl.CFBundleDisplayName`, 2026-08-30 · high. Apple
    laat deze paneelnaam in het Nederlands Engels. In lopende tekst blijft het gewone zelfstandig naamwoord wél
    Nederlands: `het juiste Apple-account`.
  - **General → `Algemeen`** · `nl/macOS/SystemSettings/Localizable.json` `GENERAL` · high.
  - **Login Items & Extensions → `Inlogonderdelen en extensies`** · macOS 26.6.2 (25G83),
    `LoginItems.appex/Contents/Resources/Localizable.loctable` `nl["Login Items & Extensions"]`, 2026-08-30 · high. ❌
    NIET `Inloggen`: dat is de oudere, kortere paneelnaam uit `InfoPlist.loctable`.

### De twee Apple-items in de menubalk

`menu.app.showAll` / `menu.app.hideOthers` (en hun tweelingen `commands.appShowAll.label` /
`commands.appHideOthers.label`) → **`Toon alles`** / **`Verberg andere`** · macOS 26.6.2 (25G83),
`Finder.app/Contents/Resources/nl.lproj/MenuBar.strings` `300730.title` / `300729.title`, 2026-08-30 · high. Apple's
eigen woorden, en meteen ook de kale gebiedende wijs die `style.md` voor knoppen en menu-items voorschrijft. De
`menu.*`-familie rendert zonder ICU, dus een apostrof schrijf je daar één keer.

## Een half teruggedraaide bewerking afmaken (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- **`Finish rolling back` → `Voltooi terugdraaien`** · macOS Finder `nl` geeft precies deze vorm: `NE108` („Finish
  Copying” → „Voltooi kopiëren”) is een bare-stem imperatief `Voltooi` plus een infinitief als lijdend voorwerp (Tier 1,
  `nl/macOS/Finder/LocalizableMerged.json`, nagekeken 2026-08-30). `terugdraaien` is het al vastgelegde werkwoord uit
  `operationLog.dialog.rollBack` („Terugdraaien”) · `high`. Het zegt afmaken, nooit opnieuw beginnen;
  `Verder terugdraaien` was het alternatief, maar dat belooft alleen doorgaan, geen einde.
- **De waarde moet identiek blijven in `operationLog.dialog.finishRollBack` en
  `fileOperations.rollbackConfirm.finishRollBack`.** Beide vertalen hetzelfde Engelse `Finish rolling back` (sourceHash
  `dbe3771`), dus `i18n-terms` slaat aan zodra één van de twee wordt bijgeschaafd. Wijzig ze samen of geen van beide.
- **`Finish rolling this back?` → `Het terugdraaien van deze bewerking voltooien?`** · zelfde vraagvorm in de infinitief
  als de zus `fileOperations.rollbackConfirm.title` („Deze bewerking terugdraaien?”), zoals elke ja/nee-dialoogtitel in
  deze catalogus. Het genominaliseerde `het terugdraaien … voltooien` staat zo bij Apple („Je kunt het kopiëren nu
  voltooien”, Finder `nl`) · `high`. De Engelse `this` wordt `deze bewerking`, precies zoals de zus hem oplost.
- **De toelichting onder de rij echoot `fileOperations.rollbackConfirm.bodyUndoByDeleting`** · „Cmdr heeft teruggedraaid
  wat het kon en de rest gelaten zoals die was. Bij het voltooien gaat Cmdr er nog een keer overheen en slaat alles over
  waar het nog steeds niet zeker van is.” De bijzin `slaat alles over waar het niet zeker van is` komt letterlijk uit
  `bodyUndoByDeleting`, `nog steeds` draagt het Engelse „still”, en `er nog een keer overheen gaan` is „takes another
  pass” · `high`. De zin belooft bewust geen volledige terugdraaiing.
- **`in {folder}` → `in {folder}`, identiek aan het Engels** · het Nederlands deelt het voorzetsel `in` voor een
  maplocatie, en macOS Finder `nl` schrijft het net zo („Search in ^1” → „Zoek in '^1'”, „items in ^0” → „onderdelen in
  '^0'”, `FF20.3_V1` en `PE23`) · `high`. Daarom draagt deze sleutel als enige van de vijf een
  `sameAsSourceJustification`; zonder die verantwoording meldt `i18n-coverage` hem als mogelijk onvertaald.
- **Geen aanhalingstekens om `{folder}`, anders dan `de` en `es` kozen.** De naaste buren in deze catalogus laten het
  ook bloot (`fileOperations.operationConflict.context` „Bezig in {destination}”, `viewer.saveAs.saved` „Selectie
  bewaard in {name}”), en het Engels doet het evenmin. Samen leest de rij als „Bezig met het aangemaakte verwijderen in
  Backup”.
- **Overloop nakijken in het bewerkingslogboek.** De dialoogtitel is 45 tekens tegen 25 in het Engels, en de knop
  `Voltooi terugdraaien` is 20 tegen 19, maar die knop vervangt in een lijstrij het veel kortere `Terugdraaien` (12),
  naast het label `Gedeeltelijk teruggedraaid`. Controleer beide tegen de pseudolocale voordat de locale meegaat.

## De terugdraaimelding: wat er weg is, en wat Cmdr met rust liet (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`)

Achttien sleutels. De gebruiker heeft een lopende kopie of verplaatsing met `Terugdraaien` afgebroken, en deze melding
vertelt wat het terugdraaien heeft gered. Cmdr weigert bewust iets te verwijderen of terug te zetten wat het niet kan
herkennen als het bestand dat het zelf schreef, dus een terugdraaiing mag eerlijk gedeeltelijk uitvallen. De toon is
overal **Cmdr heeft de zorgvuldige keuze gemaakt**: nooit verontschuldigend, nooit alarmerend, nooit "er ging iets mis".

- **"leave alone" (als Cmdr iets bewust niet aanraakt) → `ongemoeid laten` / `ongemoeid gelaten`** · de al verzonden
  `askCmdr.renameUndo.skipReason.*`-familie zegt precies dit voor precies deze Engelse zin, en twee van de acht
  redenregels hieronder dragen zelfs BYTE-IDENTIEK Engels met die familie · high. Niets in de pile kent het register
  (gecontroleerd op `macOS/`, `gnome-nautilus`, `kde-dolphin`, `xfce-thunar`, `double-commander` en
  `microsoft-terminology`, 2026-08-31), dus de al verzonden zus is hier de hoogste beschikbare bron.
  - ❌ NIET `overgeslagen` in de redenregels: dat is in deze catalogus al `Skipped` (`operationLog.outcome.skipped`).
    Het Engels splitst hier net zo: `Left {name} alone` in de lijst, `skips` in de belofte erboven.
- **`leftBehind` zegt `slaat alles over`, net als de bevestiging ervoor** · beide oppervlakken doen dezelfde belofte,
  dus dragen ze hetzelfde werkwoord, en het Engels doet het ook (`Cmdr skips anything it isn''t sure about`, in de
  dialoog én in de melding) · high. Volledige zin:
  `Cmdr slaat alles over waar het niet zeker van is, dus deze zijn blijven staan:`
  - ❌ NIET `met rust gelaten`: warmer, maar gemunt én het zou de melding uiteen laten vallen. Vier bullets met de ene
    vorm en één met de andere staan in ÉÉN melding onder elkaar; het verschil tussen de twee features ziet niemand ooit
    naast elkaar. De interne consistentie van de melding wint dus, en dat kan alleen met één vorm overal.
- **`reason.folderNotEmpty.named`/`.counted` moeten LETTERLIJK gelijk blijven aan
  `askCmdr.renameUndo.skipReason.folderNotEmpty.named`/`.counted`**
  (`Map {name} ongemoeid gelaten: er staat nu iets in.` en de getelde zus): het Engels is byte-identiek, dus
  `desktop-i18n-term-consistency` eist één Nederlandse weergave. Daarom draagt `named` hier `Map {name}` zonder lidwoord
  en `er staat nu iets in` in plaats van `daar staat nu iets in`. Wijzig ze samen of geen van beide.
- **"Left {name} where it is" → `{name} laten staan`**, dus NIET `met rust gelaten` · het Engels gebruikt hier een
  andere vorm dan bij de drie `alone`-redenen, omdat het onderdeel bij een verplaatsing juist op de BESTEMMING blijft in
  plaats van terug te gaan. `blijven staan` is het gevestigde beeld in deze familie
  (`rollbackConfirm.bodyUndoByMovingBack`: "dus er kan wat blijven staan") · high.
- **Elke redenregel is genderloos en getalloos gebouwd, want `{name}` kan een bestand (`het`) of een map (`de`) zijn.**
  Een voornaamwoord zou dus in de helft van de gevallen fout staan. De uitweg is steeds het voornaamwoordelijk bijwoord,
  dat voor beide geslachten én beide getallen werkt: `daar is iets aan gewijzigd`, `daar staat nu iets in`,
  `of daar iets aan gewijzigd is`. ❌ Schrijf hier nooit `het is gewijzigd` of `er staat iets in hem`.
  - Daarom dragen de `.named`- en de `.counted`-variant van elke reden EXACT dezelfde redenzin en verschillen ze alleen
    in de kop: zonder voornaamwoord is er niets meer dat met het getal hoeft mee te buigen, dus houdt de getelde variant
    één `plural`-blok over (het getelde zelfstandig naamwoord) in plaats van drie.
  - `folderNotEmpty.named` mag wél een lidwoord dragen (`De map {name}`), want het Engels noemt daar zelf het soort
    onderdeel, en `map` is een de-woord.
- **"after Cmdr put it there" → `nadat Cmdr er klaar mee was`** · `er … mee` is genderloos en getalloos, waar
  `nadat Cmdr het daar had neergezet` een geslacht zou kiezen · `tentative`. De plaatsbepaling ("there") valt weg; wat
  telt is dat de wijziging ná Cmdrs werk kwam, en dat blijft staan.
- **"Cmdr couldn't check whether it changed" → `Cmdr kon niet controleren of daar iets aan gewijzigd is`** ·
  `controleren` is het gevestigde werkwoord voor een controle in deze catalogus (`Controleren op wijzigingen`,
  `Bezig met controleren op conflicten`) en `gewijzigd` het gevestigde deelwoord voor een veranderd bestand
  (`Laatst gewijzigd`) · high.
  - **`controleren` geldt voor BEIDE families.** `askCmdr.renameUndo.skipReason.unverifiable.named`/`.counted` zeiden
    `Cmdr kon niet nagaan of …` en zijn op 2026-09-02 meegetrokken; `controleren` staat er nu 99 keer tegenover de twee
    `nagaan` die weg zijn. Het Engels van de twee sleutels veranderde niet, dus hun `sourceHash` bleef staan.
  - ⚠️ **Geen enkele check bewaakt dit paar, deze regel is de enige bewaker.** Het Engels van de twee families verschilt
    alléén in het apostrofteken (`couldn''t` in `fileOperations`, `couldn’t` in `askCmdr`), dus
    `desktop-i18n-term-consistency` ziet twee verschillende bronzinnen en legt ze nooit naast elkaar. Een terugval naar
    `nagaan` zou dus groen door de build komen.
  - `nagaan` blijft wél staan in `queryUi.results.live.resolvingCoverage`: daar zegt het Engels "Working out what's
    already indexed", geen "check", dus dat is een eigen keuze en geen restant van deze divergentie.
- **"Couldn't undo {name}" → `{name} terugdraaien lukte niet`** · de catalogusbrede weergave van "couldn't X" is
  `X lukte niet` (`Bundel bewaren lukte niet`, `{volumeName} uitwerpen lukte niet`, `Je notitie toevoegen lukte niet`),
  en `terugdraaien` is het vastgelegde werkwoord van deze hele familie (Microsoft bevestigt `roll back` →
  `terugdraaien`) · high. Geen `fout` en geen `mislukt`, volgens de stemregel.
  - `Its drive may be …` → `De schijf is misschien niet verbonden of alleen-lezen.` · `hun schijf is niet verbonden`
    staat al in `fileOperations.trash.undoUnavailable`, en `alleen-lezen` is macOS + Microsoft · high. Het lidwoord
    vervangt het Engelse bezittelijk voornaamwoord, zodat de enkelvouds- en de meervoudsvariant dezelfde zin delen.
- **De kop van een volledige terugdraaiing begint met `Alles`, niet met het getal.** Het Engels zegt "the {countText}
  items", maar het Nederlands kan geen bepaald lidwoord vóór een telwoord zetten (`De 1 onderdeel` is fout), dus staat
  het getal in een bijstelling achter de dubbele punt: `Alles wat Cmdr had geschreven, is verwijderd: {countText} …` en
  `Alles is teruggezet: {countText} …`. Zo klopt de zin bij élk getal en blijft de belofte ("dit was alles") overeind.
  De `some*`-broers dragen juist géén `Alles` en beginnen met het deelwoord (`{countText} onderdelen verwijderd.`),
  precies zoals `operationLog.summary.delete` · high.
- **`verwijderen` (weghalen) en `terugzetten` (op de oude plek terugbrengen) blijven strikt uit elkaar**, ook in de
  `Gestopt na het …`-koppen. `terugzetten` is Finders `Put Back` (`fileOperations.trash.undone` gebruikt het al) en
  `verwijderen` is macOS' `Delete`. ❌ Gebruik `terugdraaien` nooit voor één onderdeel, behalve in `reason.failed.*`,
  waar het Engels zelf van redenzin naar handelingszin overschakelt.
- **"The rest are still there" → `De rest staat er nog.`** en **"The rest stayed where the move put them" →
  `De rest staat nog op de nieuwe plek.`** · `de rest` is enkelvoud in het Nederlands, dus `staat` · high op `de rest`,
  `tentative` op `op de nieuwe plek`. `waar de verplaatsing ze had neergezet` viel af: een verplaatsing als handelend
  onderwerp is geen Nederlandse UI-vorm, en `de bestemming` zou technischer klinken dan het Engels.
- **`rollbackConfirm.body` erft de derde zin letterlijk van `bodyUndoByDeleting`** ("Cmdr slaat alles over waar het niet
  zeker van is, dus er kan wat achterblijven."), zoals de `@key` vraagt. De eerste twee zinnen blijven ongewijzigd; die
  waren in de terugdraaironde van 2026-08-13 al beslist.

**Overloop nakijken op de melding.** De redenregels lopen 75–90 tekens tegen 50–60 in het Engels, en
`reason.failed.counted` is de langste van de achttien. Controleer ze tegen de pseudolocale in een smalle melding.

### `cancelRollback.stagedLeftover.*` (Cmdrs eigen restanten op de bestemming)

Nieuw op 2026-09-02. Twee regels over een werkbestand dat Cmdr zelf heeft aangemaakt en niet van de bestemming
weggekregen heeft. Ze horen NIET bij de `reason.*`-lijst: daar beschermt Cmdr de bestanden van de gebruiker, hier gaat
het om een restant van Cmdr zelf.

- **`unfinished copy` → `onvolledig exemplaar`** · `onvolledig` is Apples woord voor „incomplete" (macOS `LA33`:
  „beschadigd of onvolledig"), en macOS `NE111` gebruikt `exemplaar` als zelfstandig naamwoord voor een kopie („een
  hervatbaar exemplaar bewaren") · `high`
- **`at the destination` → `op de bestemming`** · het woord uit de catalogus (`conflictsUnknown`) · `high`
- **`transfer` (zelfstandig naamwoord) → `overdracht`** · de catalogus zegt het al zo
  (`errors.listing.deviceReconnecting.explanation`: „na een geannuleerde of onderbroken overdracht") · `high`
- **`Cmdr clears it` → `Cmdr ruimt het op`** · `opruimen` in plaats van `verwijderen`, omdat het Cmdrs eigen werkbestand
  is · `high`
- ⚠️ **`bij een latere overdracht`, ❌ nooit „de volgende keer".** Cmdrs opruiming slaat alles over dat jonger is dan
  een uur, dus een directe tweede poging ruimt niets op. Een belofte die niet uitkomt is precies de fout die deze regel
  wegneemt.

## Het blokkeerscherm bij te oude WebKit (`main.oldWebkit.*`)

Drie strings die Cmdr toont in plaats van zijn interface als de Safari van de Mac te oud is. Ze staan in het
HTML-omhulsel, niet in de app, dus dit is het enige wat die persoon van Cmdr ziet.

- **`Software Update` → `Software-update`** (met streepje) · macOS noemt het paneel zo; het Tier-1-spoor uit Finder
  bevestigt de schrijfwijze (`Apple Device Software Update File` → `Software-updatebestand Apple apparaat`) · `high`.
- **`Quit` → `Stop`** · al vastgelegd in `terms.json` (macOS gebruikt `Stop`, niet `Afsluiten`) · `high`.
- **`Safari`, `Mac` en `15.4` blijven staan.** `Safari` staat nu in `BRAND_WORDS`.

## De melding over een oude macOS (`main.oldMacos.*`)

Een eenmalig dialoogvenster op een Mac onder macOS 12: Cmdr start wel, maar valt buiten het geteste bereik. Toon:
eerlijk en ontspannen, geen excuus en geen waarschuwing, want de app doet het gewoon.

- **`supported` → `ondersteund`** · macOS Finder (`… omdat deze niet wordt ondersteund.`) · `high`.
- **`X and up` → `X en nieuwer`** · macOS SystemSettings (`… is OS X %@ of nieuwer vereist.`) · `high`.
- **`best effort` → `doet het zijn best zonder garanties`** · het pile kent de term niet (alleen QoS-definities voor
  netwerken) · `high` voor de omschrijving. Bewust geen leenvertaling.
- **`look off` → `er raar uitzien`** · alledaags, en het vermijdt `fout`/`mislukt`, die de stem verbiedt.
- **Geen `z''n`**: de samentrekking zou een ICU-apostrof nodig hebben, en `zijn best` leest even natuurlijk.
- **De laatste zin is David in de ik-vorm**, met `je`, net als `onboarding.stepBeta.greeting`.

## Ask Cmdr looks inside files: the `inspectFile` tool line and the reworded consent screen (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

Five keys, mined `_ignored/i18n/nl/` (macOS Finder/AppKit, Microsoft `DUTCH.tbx`, Nautilus, Thunar, Dolphin, Total
Commander, Double Commander). The consent paragraph `contentsRule` replaces the old `noContents`; its photo-search and
approval sentences are carried over from that key verbatim so the screen keeps one voice.

- look inside files (tool line, `inspectFile.doing` / `.done`) → `Kijkt in bestanden` / `In bestanden gekeken` · no pile
  precedent (AI tool-status lines), coined in the shape of the settled siblings (`Doorzoekt je foto''s` /
  `Je foto''s doorzocht`, `Haalt mapinhoud op` / `Mapinhoud opgehaald`): present tense without subject for doing,
  participle-led for done. Bare plural `bestanden` keeps it plural-neutral (one call covers up to 200 files) · tentative
- thumbnail → `miniatuur` (plural `miniaturen`) · macOS Finder ("klein/normaal/groot formaat miniaturen",
  "Miniatuurgrootte:"), Microsoft terminology ("thumbnail"→"miniatuur", NLD/BEL), Nautilus/Thunar/Double Commander all
  "miniaturen"; already in the old `noContents` · high
- camera details (a photo's EXIF: make, model, exposure, and so on) → `cameragegevens` · compound of camera→`camera`
  (Microsoft terminology "camera"→"camera"; macOS Finder "camera's"; in-catalog "camera''s") + `gegevens`, the word this
  catalog already uses for a file's metadata (`Bestandsgegevens, niet de inhoud`, `de bestandsgegevens`). Nautilus names
  the individual fields "Merk camera" / "Model camera", so `camera` is the settled head; no source has a collective noun
  for the whole block · tentative
- location (where a photo was taken) → `locatie` · macOS Finder ("Location"→"Locatie", "Get Location"→"Haal locatie
  op"), Microsoft terminology ("location"→"locatie"), Nautilus/Thunar/Dolphin "Locatie" · high
- where it was taken (of a photo, spelled out) → `waar die gemaakt is` · standard Dutch takes a photo with `maken`
  (in-catalog `foto''s … kopiëren`, macOS "Foto's"); `die` refers back to `foto` (de-word) · high
- page (of a PDF) → `pagina` (plural `pagina''s`, ICU-doubled apostrophe) · macOS Finder ("Webpagina's"), Dolphin ("Page
  Count"→"Aantal pagina's"), in-catalog `Pagina omhoog` · high. Microsoft's "page"→"page" is the memory-paging sense
  (mining trap 4), not this one.
- PDF pages → `PDF-pagina''s` · brand+hyphen+noun compound like `Zip-archief`, `Finder-tag` · high
- title and author (PDF document metadata) → `titel en auteur` · Nautilus ("Title"→"Titel"), Dolphin
  ("Author"→"Auteur"), Microsoft terminology ("author"→"auteur") · high. Microsoft's "title"→"functie" is the job-title
  sense (trap 4); `titel` is the document sense and matches in-catalog `Chattitel`.
- archive (zip/tar/7z, and what's inside it) → `archief`; "the list of files inside an archive" →
  `de lijst met bestanden in een archief`; "what's inside an archive" → `wat er in een archief zit` · settled term
  (macOS "Zip-archief", "Soort is Archief"); the `in een archief` shape follows in-catalog
  `Er is geen prullenmand in een archief` · high
- some text / some lines of text → `wat tekst` / `wat regels tekst` · text→`tekst` is macOS Finder ("Text"→"Tekst",
  "Platte tekst"); the informal quantifier `wat` matches the `je`-register of the screen · high on `tekst`, tentative on
  `wat`
- a limited part of it → `een beperkt deel ervan` · plain Dutch; `ervan` keeps the referent (a file) neutral · high
- photo search (the feature) → `de fotozoekfunctie` · inherited from the old `noContents` key · tentative (no pile
  precedent for the feature name; the consent screen is its only prose mention)
- provider (AI) → `aanbieder` · settled term, reused ("naar je aanbieder") · high
- "nothing happens to a file until you approve it" → `er gebeurt niets met een bestand tot jij het goedkeurt` ·
  inherited verbatim from the old `noContents`; stressed `jij` marks the contrast (the assistant proposes, you decide),
  per the style guide's `jij`-for-emphasis rule · high
- "Cmdr never sends whole files, photos, or thumbnails" → `Cmdr verstuurt nooit hele bestanden, foto''s of miniaturen` ·
  send→`versturen` settled; `hele bestanden` (whole files) deliberately replaces the old key's `geen bestandsinhoud`,
  which promised no contents at all and would now be false · high
- `askCmdr.empty.hint` and `settings.askCmdr.intro` drop the old "never file contents" / "alleen-lezen" promise. "looks
  inside a file only when you ask about it" → `kijkt alleen in een bestand als je ernaar vraagt`; "never changes a file
  without your approval" → `verandert nooit een bestand zonder jouw goedkeuring` (stressed `jouw` marks the contrast) ·
  high. The first sentences of both keys stay verbatim.
- "Cmdr can suggest renames, moves, and cleanups" →
  `Cmdr kan voorstellen om namen te wijzigen, bestanden te verplaatsen en op te ruimen`.

## De twee tooltips van de Rollback-knop (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

Nieuwe surface: de tooltip zegt nu wat DEZE terugdraaiing met de bestanden doet, en de knop gaat uit zodra een
verplaatsing tussen twee bestandssystemen bij de laatste stap is (de originelen verwijderen, terwijl alles al op de
bestemming staat).

- **`rollbackTooltipStopAndMoveBack` → `Stop en zet elk bestand terug dat tot nu toe is verplaatst`** · het zusje
  `rollbackTooltip` geeft het kader (`Stop en …`), en `terugzetten` is het vaste werkwoord voor terug naar de oude plek
  (`cancelRollback.doneMovingBack`: „Alles is teruggezet”) · `high`. ❌ Niet `verwijderen`: het terugdraaien van een
  verplaatsing verwijdert niets.
- **`rollbackAlreadyLandedTooltip`** · de eerste zin volgt `cancelRollback.moveAlreadyLanded` („staat al op de
  bestemming”), `terugdraaien` is de vaste term voor rollback (`rollbackUnavailableTooltip`), en `Annuleer` is het label
  van de knop ernaast (`fileOperations.button.cancel`), dus het staat er onveranderd in · `high`.

## ‘Terminal hier openen’ en de app-keuze (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

Nieuw oppervlak: een kaart in `Gedrag > Navigatie en bewerkingen` waar je kiest welke terminal-app het commando opent.
macOS bouwt de lijst; hier worden alleen de labels vertaald.

- **terminal (de soort app) → `terminal`; Terminal (de app van Apple) → `Terminal`** · het Nederlandse macOS van Apple
  houdt de naam Engels (`Open in Terminal`, sleutel `N67` in `macOS/Finder/LocalizableMerged.json`), en het generieke
  Nederlandse woord is hetzelfde leenwoord · `high`. Daarom draagt de kaarttitel
  `settings.navigationAndFileOps.card.terminal` een `sameAsSourceJustification`: hij is met opzet gelijk aan het Engels.
- **Open terminal here (de commandonaam) → `Terminal hier openen`** · gebouwd op Apples `Open in Terminal`, met `hier`
  voor de plek · `high`. De vertaling van het commando zelf (menu, commandopalet) moet precies deze vorm gebruiken.
- **Choose an app… → `Kies app…`** · letterlijk Apples eigen `Choose Application…` (sleutel `N137`) in de Nederlandse
  Finder, die al `app` zegt · `confirmed`.
- **terminal app → `terminal-app`** · streepje, zoals de rest van de catalogus · `high`. Geen apostrof in de waarden.

## `Sort by relevance`: de tooltip van de zoekresultatenkolom (`fileExplorer.columns.sortByRelevance`)

Nieuw oppervlak: de tooltip op de actieve kolomkop van een paneel met zoekresultaten. De volgende klik zet de rijen
terug in de volgorde van de zoekmachine, met de beste overeenkomst bovenaan.

- **relevance (hoe goed een resultaat bij de zoekopdracht past) → `relevantie`** · alle vier de macOS-bronnen zijn het
  eens: WorkflowKit (`Relevance (WFSearchSortOrder)` → `Relevantie`), AppStoreKit (`SEARCH_FACET_RELEVANCE` →
  `Relevantie`), Automator (`%1$[Relevantie]@ …`) en Muziek · `high`. Kleine letter na `op`, zoals elders in de
  catalogus. (gecontroleerd op macOS 26.6.2, build 25G83, `plutil`-uitvoer van de meegeleverde lokalisaties, 2026-09-06)
- **Zinsframe → `Sorteer op relevantie`** · precies het patroon van de zustersleutels in `commands.json`
  (`Sorteer op naam`, `Sorteer op grootte`) · `high`. Geen `sameAsSourceJustification`, en de waarde bevat geen
  apostrof.

## `Documents and packages`: de nieuwe OOXML-rij (`settings.archives.ooxml.*`)

Nieuw oppervlak: een rij in dezelfde kaart als `Zip-archieven`, boven de kaart `App-pakketten`. De rij dekt met opzet
ALLEBEI: Office-documenten (.docx, .xlsx, .pptx) en app-pakketten (.jar, .apk). Daarom noemt zelfs het Engels Office
niet.

- **documents (het bestandssoort) → `Documenten`** · macOS Finder (`TL6`/`GROUP_DOCUMENTS` → `Documenten`; soorten
  `RTF-document`, `Platte-tekstdocument`) · `high`.
- **packages (generiek, niet alleen apps) → `pakketten`** · macOS Finder (`Toon pakketinhoud`) en de term
  `app bundle → pakket` · `high`. Bewust het kale `pakketten`, zodat de rij breder blijft dan de kaart `App-pakketten`
  eronder — dezelfde scheiding die het Engels maakt met `packages` tegenover `app bundles`.
- **Zinsframe → `Wat Enter doet bij een …, … of ….`** · precies het frame van de zustersleutels
  `settings.archives.zip.description` en `settings.archives.bundle.description` · `high`. Geen apostrof in de waarde.

## De serverhub: verbindingsscherm, verbindingstooltips en de vergeet-bevestigingen (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `.disconnect*`, `.forget*`)

Nieuw oppervlak: het paneel dat je ziet terwijl Cmdr een server opent (of weigert te openen), de tooltip op het
verbindingsbolletje van een serverrij in de volumeschakelaar, en de twee bevestigingsvensters die een server of het
bewaarde wachtwoord vergeten. De referentiestapel ontbreekt op deze machine, dus Tier 1 komt uit de LIVE macOS-bundels
(`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?"), alles geverifieerd op macOS 26.6.2, build
25G83, 2026-09-06. Tier 2 (Microsoft) was onbereikbaar; wat alleen daarmee te beslissen viel, staat op `tentative`.

Termen die uit de bundels kwamen:

- **Keychain Access (de app) → `Sleutelhangertoegang`** · `Keychain Access.app/Contents/Resources/InfoPlist.loctable`,
  `nl` → `CFBundleName: "Sleutelhangertoegang"` · `high`. De catalogus schreef dit al zo in
  `ai.secretError.keychainBody`, dus de nieuwe zin sluit daarop aan. `Keychain` alleen blijft `Sleutelhanger`
  (`servers.sheet.remember` = `Onthoud in Sleutelhanger`).
- **Connecting to X… → `Verbinden met {name}…`** · Finder `LocalizableMerged` `MN1` (`Verbinden met '^0'…`) en
  NetAuthAgent `Localizable.loctable` `CONNECTING_TO_HOST` (`Verbinden met '%@'.`) · `high`. Geen aanhalingstekens om
  `{name}`, omdat het Engels ze ook niet heeft.
- **server address → `serveradres`** · Finder `ConnectToWindow.strings` `YEA-3L-WnW.placeholderString` (`Server Address`
  → `Serveradres`) · `high`. Eén woord, zoals elk Nederlands samengesteld zelfstandig naamwoord.
- **certificate → `certificaat`; to trust → `vertrouwen`; not trusted → `wordt niet vertrouwd`** · Keychain Access
  `InfoPlist.loctable` (`certificate` → `certificaat`) plus de systeembrede patronen
  (`Deze website wordt niet vertrouwd en doet zich mogelijk voor als %@`, `Het pakket '%@' wordt niet vertrouwd`) ·
  `high`.
- **SSH-sleutel, `known_hosts`, SSH-instellingen** · Apple schrijft `SSH key` → `SSH-sleutel` en
  `your SSH known_hosts file` → `het known_hosts-bestand voor SSH` (Opdrachten/Schermdeling `ScreenSharing.loctable`
  `sshTunnelHostKeyChangedMessage`) · `high`. In deze pass gaat het Engels niet verder dan `key`, dus de waarde zegt
  gewoon `de sleutel van {host}`; `SSH settings` wordt `je SSH-instellingen`, met het al vaste `instellingen`.
- **didn't answer in time → `reageerde niet op tijd`** · niet nieuw, maar hier hergebruikt uit
  `errors.volume.connectionTimeout` (`De verbinding reageerde niet op tijd`) · `high`.

Vormen die uit de eigen catalogus kwamen, niet uit de bundels (consistentie wint van een frisse keuze, en
`desktop-i18n-term-consistency` telt dezelfde Engelse bron als één term):

- `Disconnect` → **`Verbreek`**, byte-identiek aan `menu.network.disconnect`, `fileExplorer.unreachable.disconnect` en
  `servers.paneState.disconnect` (en aan Finder `MR10.1` / `N200`).
- `Cancel` → **`Annuleer`** (18 zusjes, plus NetAuthAgent `CANCEL`), `Try again` → **`Probeer opnieuw`** (5 zusjes).
- `Forget server` → **`Vergeet server`** en `Forget saved password` → **`Vergeet opgeslagen wachtwoord`**, gelijk aan
  `menu.network.forgetServer` / `menu.network.forgetSavedPassword` / `fileExplorer.network.share.forgetPassword`. De
  bevestigingstitel is hetzelfde Engels als het menu-item dat het venster opent, dus hij is ook in het Nederlands
  hetzelfde: de gebruiker klikt op `Vergeet server` en leest `Vergeet server`.
- **`disconnectBusyTooltip` volgt zijn zusje letterlijk.** `ejectBusyTooltip` luidt
  `Uitwerpen kan niet terwijl er bewerkingen op dit apparaat bezig zijn`, dus deze wordt
  `Verbinding verbreken kan niet terwijl er bewerkingen op deze server bezig zijn`. Eén patroon, twee slots.
- **De bevestigingsvraag staat in de infinitief-eindvorm**, zoals `fileExplorer.network.browser.removeHostConfirm`
  (`{hostName} uit de serverlijst verwijderen?`) en `settings.mediaIndex.clip.deleteConfirmTitle`. Dus
  `{name} vergeten?` en `Het opgeslagen wachtwoord voor {name} vergeten?`, met de knop ernaast in de imperatief.
- **`stops listing it` → `haalt de server uit de lijst`** · `serverlijst` is al de term in `removeHostConfirm`, en de
  serverrij verdwijnt letterlijk uit die lijst · `high`.

De ARIA-regel: **`disconnectPlaceAriaLabel` → `Verbreek de verbinding met {name}`**. Het zichtbare label van deze actie
is `Verbreek` (`servers.paneState.disconnect`, `fileExplorer.unreachable.disconnect`), en dat woord staat als eerste
woord letterlijk in de toegankelijke naam, dus WCAG 2.5.3 (Label in Name) klopt. Het letterlijke zusjespatroon
(`ejectVolumeAriaLabel` → `Werp {name} uit`) kan hier niet: je verbreekt in het Nederlands een verbinding, geen server.
De zin volgt daarom `servers.paneState.disconnectCycleTooltip` (`… en verbreek de verbinding met de server`).

Nieuw gemunte vormen, zonder bron in een bundel:

- **sign-in method → `inlogmethode`** · de catalogus zegt overal `inloggen` / `ingelogd` / `Log in`
  (`fileExplorer.network.signIn`, `.browser.status.loggedIn`), dus dit is het consistente samengestelde woord ·
  `tentative` (geen Apple- of Microsoft-bron; Microsoft was onbereikbaar op deze machine).
- **Signed out → `Uitgelogd`** · de tegenhanger van het al aanwezige `Ingelogd`
  (`fileExplorer.network.browser.status.loggedIn`) · `high`.
- **compromised (een sleutel die als onveilig gemarkeerd staat) → `gecompromitteerd`** · Apple heeft dit woord niet: het
  komt bij een certificaat op `ingetrokken` uit en bij privacy op `in gevaar gebracht`, en geen van beide zegt wat
  `@revoked` in `known_hosts` bedoelt. `gecompromitteerd` is het gangbare Nederlandse beveiligingswoord · `tentative`,
  zie `review-queue.md`.

## De serverhub (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`, `shortcuts.scope.servers`/`.places`)

Achtentwintig sleutels voor de nieuwe serverhub: de rij `Servers` in de volumekiezer opent een tabel met elke opgeslagen
server (SFTP, WebDAV, SMB) plus de servers die Cmdr in de buurt vindt, met kolommen Naam / Type / Adres / Status /
Laatst gebruikt en onderaan een rij `Voeg server toe…`. De GROEP waarin die rij staat blijft `Netwerk`
(`fileExplorer.navigation.groupNetwork`).

De referentiestapel (`_ignored/i18n/nl/`) ontbreekt op deze machine (de M1-agentbox), dus Tier 1 is gemijnd uit de LIVE
macOS-bundels volgens `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?". Alles geverifieerd op
macOS 26.6.2, build 25G83, 2026-09-06. Tier 2 (Microsoft) was onbereikbaar, dus een term die Microsoft nodig had om een
knoop door te hakken blijft `tentative`.

Termen:

- **server → `server`; servers → `servers`** · macOS AppKit `Menus.loctable` (`Servers`→`Servers`), Finder `nl`
  `ConnectToWindow.strings` (`Favoriete servers:`, `Wis recente servers…`), Finder `LocalizableMerged`
  (`Connect to Server…`→`Verbind met server…`) · high. Gelijk aan het Engels, dus `sameAsSourceJustification` op
  `fileExplorer.navigation.networkVolume`, `shortcuts.scope.servers` en `servers.hub.rowCount`.
- **Locations (het kopje boven de plekken onder één server) → `Locaties`** · macOS Finder `LocalizableMerged`
  `SD5`/`FI9` (`Locations`→`Locaties`), het kopje van de navigatiekolom · high. Het Engels zegt hier `Places` in plaats
  van `Locations`, maar het Nederlands heeft geen tweede kort woord voor hetzelfde begrip, en `Locatie` is al de
  gevestigde vertaling van `location` in deze catalogus.
- **Address (kolomkop) → `Adres`** · macOS Network-paneel (`Server Address:`→`Serveradres:`, `IP Address`→`IP-adres`),
  en `serveradres` staat al in de catalogus (`servers.refusal.invalidUrl` = `Dat lijkt geen serveradres te zijn.`) ·
  high.
- **Type (kolomkop, het protocol) → `Type`** · macOS Systeeminstellingen `Localizable.loctable` (`Type`→`Type`) · high.
  ❌ NIET `Soort`: dat is Finders kolom `Kind` voor een bestandssoort, en Apple zelf laat `Type` staan.
- **Status (kolomkop) → `Status`** · macOS Systeeminstellingen `Localizable.loctable` (`Status`→`Status`) · high.
- **Last used → `Laatst gebruikt`; Never (in die kolom) → `Nooit`** · macOS Systeeminstellingen `Localizable.loctable`
  (`Last Used`→`Laatst gebruikt`, `Never`→`Nooit`) · high.
- **Connected (status) → `Verbonden`** · macOS AppKit `SavePanel.loctable` + Systeeminstellingen
  (`Connected`→`Verbonden`), al in de catalogus (`fileExplorer.network.browser.status.connected`) · high.
- **Saved (status: bewaard, nu niet verbonden) → `Opgeslagen`** · de catalogus zegt het al voor precies deze toestand
  (`fileExplorer.navigation.connectionTooltipSaved` = "Opgeslagen. Open de server om te verbinden.") · high.
- **Signed out (status) → `Uitgelogd`** · de tegenhanger van `Ingelogd`, en de catalogus gebruikt het al voor deze
  toestand (`fileExplorer.navigation.connectionTooltipNeedsSignIn` = "Uitgelogd. Open deze server om opnieuw in te
  loggen.") · high. Het is geen weigering en geen fout, en `Uitgelogd` draagt dat neutraal.
- **Found nearby (status) → `Gevonden in de buurt`** · Apples vaste weergave van `nearby` is `in de buurt` (Finder
  AirDrop: "anderen bij jou in de buurt"; Systeeminstellingen: "persoonlijke hotspots in de buurt") · high op
  `in de buurt`, `tentative` op de woordvolgorde. `In de buurt gevonden` bestaat ook; het deelwoord vooraan leest als
  een zelfstandig label, zoals Finders `Gedeeld door`.
- **host key (de identiteitssleutel van een SSH-server) → `de sleutel`** · al gevestigd in de catalogus
  (`servers.refusal.hostKeyUntrusted` "Cmdr vertrouwt de sleutel van {host} nog niet",
  `fileExplorer.navigation.connectionTooltipNeedsHostKey`) · high. Geen `hostsleutel`: de context zegt al over welke
  sleutel het gaat.
- **"Waiting for you to check the key" → `Wachten tot je de sleutel controleert`** · de gevestigde wachtvorm
  `Wachten tot <bijzin>` (zie § Vastgelopen overdracht) met `controleren` in plaats van `bekijken`, omdat de gebruiker
  de vingerafdruk vergelijkt en niet alleen bekijkt (Finder `NE18` "controleer") · high.
- **local network → `lokaal netwerk`; local network discovery → `lokale netwerkdetectie`** · macOS Systeeminstellingen,
  privacypaneel (`Local Network`→`Lokaal netwerk`) voor het eerste deel; `detectie` komt uit de catalogus zelf
  (`settings.network.firstTriggerDone.label` = "Netwerkdetectie gestart") en uit Apples
  `Auto proxy discovery`→`Automatische proxydetectie` · high op beide delen, `tentative` op de samenstelling
  `lokale netwerkdetectie`, die nergens als geheel voorkomt.
- **"… is off." (een functie die in Instellingen uitgezet is) → `… staat uit.`** · de gevestigde vorm in deze catalogus
  voor precies dit patroon (`driveIndex.tooltipDisabled` "Het indexeren staat uit voor deze schijf",
  `.tooltipIndexingOff`) · high. De termregel `turned on → ingeschakeld` gaat over het instellingenlabel zelf, niet over
  deze verwijzing van elders.
- **"Turn it on in Settings" → `Zet het aan in Instellingen`** · dezelfde zusjesregel als
  `driveIndex.refusedIndexingOff` ("Zet het aan bij Indexeren > Schijf indexeren"); `Settings` → `Instellingen` is macOS
  Tier 1 · high.
- **Edit … (een formulier openen om iets te wijzigen) → `Wijzig …`** · macOS Network-paneel (`Network.appex`:
  `Edit…`→`Wijzig…`, `Edit`→`Wijzig`) en AppKit (`Edit`→`Wijzig`) · high. ❌ NIET `Bewerk`: dat is in deze catalogus
  gereserveerd voor een bestand in een editor openen (`commands.fileEdit.label`, `menu.file.edit`).
- **volume switcher → `volumekiezer`** · het Engels noemt dezelfde UI afwisselend `volume switcher` en `volume chooser`,
  en de catalogus geeft die al één Nederlandse naam (`commands.volumeClose.label` = "Sluit volumekiezer",
  `shortcuts.scope.volumeChooser` = "Volumekiezer") · high. Het oude rijtje `switcher → wisselaar` blijft ongebruikt: de
  gebruiker ziet `volumekiezer`.
- **pin / unpin (een server in de volumekiezer zetten of eruit halen) → `vastzetten` / `losmaken`; opdracht
  `Zet server vast / maak hem los`** · Safari `nl` (`Maak tabblad vast` / `Maak tabblad los`) en het rijtje
  `pin (tab) → vastzetten` · high op het werkwoordpaar, `tentative` op de labelvorm. De schuine streep blijft omdat één
  opdracht beide richtingen doet; `hem` verwijst naar `de server` en is dus veilig (het is geen ongecontroleerde
  invoeging).
- **"Add server…" → `Voeg server toe…`** · de gevestigde `Voeg … toe`-vorm met het partikel achteraan (macOS Finder
  `Voeg wachtwoord toe`, `Voeg tags toe`) · high.
- **"No servers yet" → `Nog geen servers`** · de `Nog (niet|geen) …`-vorm die de catalogus al gebruikt
  (`settings.mediaIndex.networkVolumes.notIndexedYet` = "Nog niet geïndexeerd") · high.
- **NAS → `NAS`** · onvertaald; de catalogus gebruikt het al (`settings.network.smbConcurrency.description` = "de meeste
  NAS-hardware thuis") en het is in het Nederlands het gangbare woord · high.

Notities:

- De drie meldingen noemen de server bij naam via `{name}`, een ongecontroleerde invoeging. `serverUnpinnedToast` zegt
  daarom `De server is nog steeds opgeslagen.` in plaats van een voornaamwoord, en `pinRefusedToast` gebruikt
  `waar {name} te zien is`, dat bij elk geslacht en elk getal klopt. Zie § Notities en beslissingen in `style.md`.
- `pinRefusedToast` volgt het zusjespatroon `Cmdr kon … niet …` (`disconnectRefusedToast`, `forgetServerRefusedToast`,
  `forgetSecretRefusedToast`), dus geen `fout` en geen `mislukt`.
- `commands.serversForgetSecret.label` is byte-identiek aan het al aanwezige
  `fileExplorer.navigation.forgetSecretConfirmTitle` en `fileExplorer.network.share.forgetPassword`
  (`Vergeet opgeslagen wachtwoord`), zodat de opdracht, het menu-item en de bevestiging één ding zeggen.
- `commands.serversDisconnect.label` gebruikt het vaste label `Verbreek verbinding` en spiegelt Finders
  `Verbind met server`: `Verbreek verbinding met server`.
- `sameAsSourceJustification` staat op vijf sleutels: `fileExplorer.navigation.networkVolume`,
  `shortcuts.scope.servers`, `servers.hub.colType`, `servers.hub.colStatus` en `servers.hub.rowCount` (het Nederlandse
  meervoud van `server` is `servers`, dus beide ICU-takken vallen samen met het Engels).

## Het serververbindingsvenster en de sleutelvraag (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

Zesenveertig sleutels voor het venster waarin je een server toevoegt of erop inlogt (SFTP, WebDAV, SMB), voor de stap
waarin je de SSH-sleutel van een server vertrouwt, en voor twee voorbeeldregels onder `Ga naar pad`.

De referentiestapel (`_ignored/i18n/nl/`) ontbreekt op deze machine (de M1-agentbox), dus Tier 1 komt uit de LIVE
macOS-bundels volgens `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?". Alles geverifieerd op
macOS 26.6.2, build 25G83, 2026-09-07. Tier 2 (Microsoft) was onbereikbaar.

Termen die letterlijk uit de bundels kwamen:

- **Add Server → `Voeg server toe`** · macOS `Localizable.loctable` (`Add Server`→`Voeg server toe`) · high. Precies de
  vorm die `servers.hub.addServer` al gebruikt, dus de venstertitel en de rij eronder zeggen hetzelfde.
- **Connect (knop) → `Verbind`; Connect to Server → `Verbind met server`** · Finder `nl.lproj/ConnectToWindow.strings`
  `46.title` en `1.title`, plus NetAuthAgent `AuthDialog.loctable` `600218.title` · high.
- **Browse… → `Blader…`** · Finder `nl.lproj/ConnectToWindow.strings` `48.title` (`Browse`→`Blader`), naast de knop die
  daar hetzelfde doet: een bladervenster openen · high. ❌ NIET `Bladeren`: dat is de infinitief die Safari's menubalk
  gebruikt, en dit is een knop (imperatiefregel in `style.md`).
- **Sign in to X → `Log in bij X`** · CloudKit `Localizable.loctable` (`Sign In to %1$@`→`Log in bij %1$@`), plus tien
  gelijkvormige zinnen (`Log in bij iCloud`, `Log in bij je Apple Account`, `Log in bij de App Store`) · high. Eén
  sleutel draagt deze kop: `servers.sheet.signInTitle` = `Log in bij {name}`, gebiedend en zonder aanhalingstekens,
  precies zoals het Engels (`Sign in to {name}`).
- **Signed out of X → `Uitgelogd bij X`** · macOS `Localizable.loctable` (`iCloud Signed Out`→`Uitgelogd bij iCloud`),
  en `Uitgelogd` was al de gevestigde toestandsnaam (`servers.hub.status.signedOut`) · high.
- **Sign In → `Log in`; Log In → `Log in`** · macOS `Localizable.loctable`, gelijk aan de catalogus
  (`fileExplorer.network.signIn`) · high.
- **Guest → `Gast`; Connect As → `Verbind als`** · NetAuthAgent `AuthDialog.loctable` `RiA-l0-ASw.title` /
  `PHL-pS-ELV.title` · high. Daaruit stelt `Connect as guest` → `Verbind als gast` zich samen; geen andere sleutel
  levert dit Engels, dus er is geen zusterwaarde om mee te pareren.
- **Remember in Keychain → `Onthoud in Sleutelhanger`** · Apples eigen zin is
  `Remember this password in my keychain`→`Bewaar wachtwoord in mijn sleutelhanger` (NetAuthAgent `AuthDialog.loctable`
  `600268.title`), wat `sleutelhanger` bevestigt · high. Ook hier levert geen andere sleutel dit Engels.
- **passphrase → `wachtzin`; Key passphrase → `Sleutelwachtzin`** · Apple schrijft overal `wachtzin`
  (`SecErrorMessages.loctable`, `P12Password.loctable` `Enter Passphrase:`→`Geef wachtzin op:`, DiskImages
  `Incorrect passphrase`→`Onjuiste wachtzin`) en maakt er samenstellingen mee
  (`A FileVault “Disk Passphrase” User is required.`→`Een FileVault-gebruiker met een schijfwachtzin is vereist.`) ·
  high op `wachtzin`, `tentative` op de samenstelling `Sleutelwachtzin`. ❌ NIET `wachtwoordzin` en niet `passphrase`.
- **remote → `extern(e)`; Remote folder → `Externe map`** · Finder `LocalizableMerged.loctable`
  (`Remote Volume`→`Extern volume`), Schermdeling (`the remote computer`→`de externe computer`), Schijfhulpprogramma
  (`the remote server`→`de externe server`) · high.
- **Trust → `Vertrouw`; Always Trust → `Vertrouw altijd`** · macOS `Localizable.loctable` · high. Dus
  `Trust and connect` → `Vertrouw en verbind` (twee gebiedende stammen naast elkaar, zoals de knopregel voorschrijft) en
  `Trust the new key` → `Vertrouw de nieuwe sleutel`.
- **fingerprint → `vingerafdruk`** · Apple localiseert dit ook voor een server-identiteit
  (`The fingerprint of the SCEP server “%@” does not match.`→`De vingerafdruk van de SCEP-server '%@' komt niet overeen.`),
  en `Fingerprint`→`Vingerafdruk` staat door het hele systeem · high, met een kanttekening: Apples eigen SSH-teksten in
  Opdrachten laten `fingerprint` juist ONVERTAALD staan (`De fingerprint van de sleutel van de host is %@.`). Die twee
  strings zijn de zwakste Apple-Nederlandse teksten die deze pass tegenkwam (`de key van de host` in de zusterzin), dus
  de systeembrede `vingerafdruk` wint. Zie `review-queue.md`.
- **Key fingerprint → `Vingerafdruk van de sleutel`** · de genitiefvorm die Apple voor precies dit gebruikt
  (`De vingerafdruk van de SCEP-server`, `De fingerprint van de sleutel van de host`) · high. Niet de samenstelling
  `Sleutelvingerafdruk`: Apple zet de bezitter achter het woord zodra het om een sleutel of server gaat.
- **Reconnect → `Verbind opnieuw`; Reconnect automatically → `Verbind automatisch opnieuw`** · macOS
  `Localizable.loctable` (`Reconnect`→`Verbind opnieuw`) plus Mail `ComposingPreferences.loctable`, waar `Automatically`
  als `Verstuur automatisch` staat: het bijwoord komt direct achter de stam, het partikel blijft achteraan · high.
- **X has stopped Y-ing → `X … niet meer …`** · Apples vaste omzetting van deze Engelse vorm
  (`This podcast has stopped updating …`→`Deze podcast wordt niet meer bijgewerkt …`,
  `%1$@ has stopped viewing the photos in your stream.`→`%1$@ bekijkt niet meer de foto's in je stream.`) · high. Dus
  `Cmdr stopped connecting to {name}` → `Cmdr verbindt niet meer met {name}`, in de tegenwoordige tijd, want het paneel
  blijft staan tot de gebruiker iets doet.
- **has changed → `is gewijzigd`** · Schermdeling `ControlCommand.loctable`
  (`The remote host identification has changed for “%@”.`→`De identificatie van de externe host is gewijzigd voor '%@'.`)
  · high. Dus `{host}''s key changed` → `De sleutel van {host} is gewijzigd`.
- **Protocol → `Protocol`** · `IP.loctable` (`Protocol:`→`Protocol:`) en `WDWiFiScan.loctable` · high, en dus een
  `sameAsSourceJustification`.
- **hostname → `hostnaam`** · nu wél gesourcet (AirPortSettings `Hostname`→`Hostnaam`, OID `Host Name`→`Hostnaam`), dus
  de oude `tentative` in de bovenste termenlijst mag op `high`.
- **Save → `Bewaar`; Advanced → `Geavanceerd`; Never → `Nooit`; Try Again → `Probeer opnieuw`** · macOS
  `Localizable.loctable`, alle vier al in de catalogus · high.

Vormen die uit de eigen catalogus kwamen (byte-identiek Engels wint van een frisse keuze):

- `Cancel` → `Annuleer`, `Password` → `Wachtwoord` (`fileOperations.archivePassword.placeholder`), `Name` → `Naam`,
  `Address` → `Adres` (`servers.hub.colAddress`), `Advanced` → `Geavanceerd` (`settings.section.advanced`), `Sign in` →
  `Log in` (`fileExplorer.network.signIn`), `Connect` → `Verbind` (`fileExplorer.network.connect`), `Connect to server…`
  → `Verbind met server…` (`settings.network.permissionIntroConnectLink`, en gelijk aan Finders `Verbind met server`).
- `Username` → `Gebruikersnaam`, `Connect as guest` → `Verbind als gast` en `Remember in Keychain` →
  `Onthoud in Sleutelhanger` staan alleen op dit venster, dus er is geen zusterwaarde om mee te pareren. De twee
  NetAuthAgent-regels hierboven dragen de laatste twee, en `gebruikersnaam` staat al in lopende tekst in
  `errors.listing.authRequiredEauth.suggestion` (`voer je gebruikersnaam en wachtwoord opnieuw in`); `needsStoredSecret`
  citeert het schakelaarlabel woord voor woord (`Zet ‘Onthoud in Sleutelhanger’ aan en log één keer in.`).

Nieuw gemunte vormen, zonder bron in een bundel:

- **Key file → `Sleutelbestand`** · de gebruikelijke Nederlandse samenstelling, in lijn met Apples `known_hosts-bestand`
  en `SSH-sleutel` · `tentative`.
- **How to connect → `Manier van verbinden`** · toegankelijke naam van de gast-of-account-keuze, alleen voor
  schermlezers, op `servers.sheet.connectionModeLegend`. Letterlijk `Hoe verbinden` leest niet; de zelfstandige groep
  wel · `tentative`.
- **server's owner → `de servereigenaar`** · gewone Nederlandse samenstelling; het alternatief
  `de eigenaar van de server` maakte de zin twaalf tekens langer in een regel die het Engels in één adem zegt ·
  `tentative`.
- **ssh line → `ssh-regel`** · `regel` is al de vertaling van `line` in deze catalogus, en `ssh` blijft kleingeschreven
  en onvertaald zoals de `@key` vraagt; koppelteken zoals bij `SMB-share` · high.
- **Sleutelwachtzin** en **Externe map**: zie hierboven.

Notities:

- **`{name}` en `{host}` zijn ongecontroleerde invoegingen**, dus geen enkel voornaamwoord verwijst ernaar.
  `changedTitle` zegt `De sleutel van {host} is gewijzigd` (lidwoord, geen `zijn/haar`), en `hostKeyChangedHint` opent
  met `De sleutel is gewijzigd.` in plaats van `Zijn sleutel …`.
- **`Disconnect` wordt in een zin altijd `Verbreek de verbinding`**, nooit `Verbreek de server`: dat is de al
  vastgelegde ARIA-regel bij `disconnectPlaceAriaLabel`. `hostKeyChangedHint` volgt hem.
- **De aanhalingstekens in `needsStoredSecret` zijn de enkele krulletjes `‘…’`** die macOS-Nederlands gebruikt, niet de
  Engelse dubbele. U+2018/U+2019 is geen ICU-escapeteken (alleen de ASCII `'` is dat), dus er is geen verdubbeling
  nodig.
- **Geen enkele van de 46 waarden bevat een ASCII-apostrof**, dus nergens ICU-verdubbeling. De ellips is overal het
  enkele teken `…` (U+2026), ook in `Blader…`, `Log in…`, `Verbinden…` en `Verbind met server…`.
- `sameAsSourceJustification` staat op vijf sleutels: `servers.sheet.protocolLegend` (Apple laat `Protocol` staan),
  `servers.sheet.protocolSmb`/`.protocolSftp`/`.protocolWebdav` (protocolnamen; macOS-Nederlands schrijft
  `SMB-wachtwoord`, `FTP-wachtwoord`, `WebDAV-wachtwoord`) en `servers.sheet.addressPlaceholder` (`nas.local` is een
  voorbeeldhostnaam: `nas` is ook in het Nederlands de gangbare afkorting, en `.local` is het mDNS-achtervoegsel dat
  macOS in elke taal onvertaald toont).
- **Mijngereedschap-valkuil, waard om te onthouden:** `grep -a "<Engelse tekst>"` vindt NIETS in een `.loctable`, ook al
  staat de tekst erin; de recept-notitie in `how-to-mine.md` suggereert van wel. `plutil`/`plistlib` uitpakken is de
  enige betrouwbare weg. Op deze machine ontbreekt `timeout` bovendien, dus een commando dat ermee begint faalt
  stilletjes met `command not found` en leest als "geen resultaten". Een `python3` + `plistlib` sweep over alle 7.185
  `.loctable`-bestanden onder `/System` levert in één minuut een `en`→`nl`-index van 271.441 paren op; dat is het
  snelste vervangende Tier 1 dat deze pass gevonden heeft.

## Twee paneelregels erbij: automatisch opnieuw verbinden en inloggen met een sleutel (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

Twee sleutels in hetzelfde serverpaneel. De ene is de kop terwijl Cmdr uit zichzelf, in een backoff-lus, een weggevallen
verbinding terughaalt (eronder: een spinner, een aftelling en de knoppen `Probeer nu opnieuw` / `Annuleer` /
`Verbreek`). De andere is de regel onder `Uitgelogd bij {name}` die in de plaats komt van de knop `Log in…`, omdat deze
server zich met een sleutel legitimeert en er dus niets in te vullen valt.

De referentiestapel ontbreekt op deze machine, dus Tier 1 komt uit de LIVE macOS-bundels
(`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?"), geverifieerd op macOS 26.6.2, build 25G83,
2026-09-07.

- **Reconnecting… → `Opnieuw verbinden…`** · letterlijk Apple, in `ScreenSharing.framework/…/ScreenSharing.loctable`
  (`Reconnecting…` → `Opnieuw verbinden…`, en `Reconnect` → `Verbind opnieuw`) · `high`. Dezelfde vorm staat al in de
  catalogus als `errors.listing.deviceReconnecting.title` (`Reconnecting to the device` →
  `Opnieuw verbinden met het apparaat`), dus `Reconnecting to {name}…` wordt **`Opnieuw verbinden met {name}…`**: het
  zusje `paneState.connecting` (`Verbinden met {name}…`) met `Opnieuw` ervoor, zodat de twee koppen als één paar lezen.
  - De hele PPP/VPN-familie bevestigt dat `reconnect` als wérkwoord `opnieuw verbinding maken` is
    (`PPPController.bundle/…/Localizable.loctable`, ~50 zinnen: `Try reconnecting.` →
    `Probeer opnieuw verbinding te maken.`). Dat is de vorm voor een volle zin; voor een voortgangskop wint Apples eigen
    `Opnieuw verbinden…`, dat korter is en het gerundium van het Engels behoudt.
- **signs in with a key rather than a password → `log je in met een sleutel in plaats van een wachtwoord`** · de shape
  komt van de eigen catalogus (`servers.sheet.signInWithCredentials`: `Log in met een gebruikersnaam en wachtwoord`), en
  Apple zegt het in een hele zin net zo (`ScreenSharing.loctable`:
  `Screen Sharing requires a password to sign in to “%@”.` →
  `Voor schermdeling moet je met een wachtwoord inloggen op '%@'.`) · `high`. Voorzetsel `bij`, niet `op`, want de
  catalogus heeft `Log in bij {name}` (`servers.sheet.signInTitle`) en `Uitgelogd bij {name}`.
- **key (de sleutel waarmee de client zich legitimeert) → `een sleutel`** · het Engels gaat hier niet verder dan `key`,
  dus de waarde ook niet; de al vastgelegde regel `SSH key → SSH-sleutel` blijft voor het Engels dat `SSH` wél noemt ·
  `high`. Zie `review-queue.md`.
- **there''s nothing to type → `er valt niets te typen`** · `typen` is het woord dat de catalogus al gebruikt
  (`ui.combobox.emptyText`: `Blijf typen om je eigen waarde te gebruiken`); `invullen` is de andere kandidaat, maar het
  Engels zegt letterlijk `type` en er is geen formulier meer om in te vullen · `tentative` (geen Apple-bron voor deze
  wending; Tier 2 was onbereikbaar).
- **Open it again to retry. → `Open de server opnieuw om het nog eens te proberen.`** · de zin neemt de vorm over van
  het zusje twee sleutels hoger, `paneState.hostKeyChangedHint`
  (`… open de server opnieuw om de vingerafdruk te controleren`), en van
  `fileExplorer.navigation.connectionTooltipNeedsSignIn` (`Open deze server om opnieuw in te loggen.`) · `high`. Het
  naamwoord staat er voluit in plaats van `hem`, omdat `hem` in deze zin net zo goed naar `de sleutel` kan wijzen.
  `nog eens` in plaats van het gebruikelijke `opnieuw`, omdat `opnieuw` al in `Open de server opnieuw` staat.

Notities: geen ASCII-apostrof in beide waarden, dus geen ICU-verdubbeling. `{name}` blijft ongewijzigd en krijgt geen
voornaamwoord; de ellips is het enkele teken `…` (U+2026). Geen `sameAsSourceJustification`: beide waarden verschillen
van het Engels.

## De vastzet-hint, de vertrouwde serversleutels en het ADB-paneel (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.appearance.tintSmb.*`)

Eenendertig sleutels: de twee contextmenu-items op een serverrij in de volumekiezer, de eenmalige melding die verschijnt
zodra er vijf servers in de groep `Netwerk` staan, de instellingenpagina met de sleutels die deze Mac van servers
vertrouwt, de instellingenpagina voor Android via ADB, en het hernoemde tintlabel dat nu ook SFTP en WebDAV dekt.

De referentiestapel (`_ignored/i18n/nl/`) ontbreekt op deze machine (de M1-agentbox), dus Tier 1 komt uit de LIVE
macOS-bundels volgens `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?". Alles geverifieerd op
macOS 26.6.2, build 25G83, 2026-09-07. Tier 2 (Microsoft) was onbereikbaar; wat alleen daarmee te beslissen viel, staat
op `tentative`.

Termen die letterlijk uit de bundels kwamen:

- **Unpin → `Maak los`** · exact dit Engels, exact deze waarde, in acht bundels tegelijk (Maps, Shortcuts, MapKit,
  NotesShared, PhotosUICore, RemindersAppIntents, ScreenReaderOutput, VideosUI, telkens `Localizable.loctable` /
  `MapKit.loctable`) · `high`. Gelijk aan het al aanwezige `menu.tab.unpinTab` (`Maak tabblad los`).
- **Pin → `Maak vast`; Pin to <plek> → `Maak vast in <plek>`** · macOS `Localizable.loctable` (`Pin`→`Maak vast`,
  `Pin in Menu Bar`→`Maak vast in menubalk`, `Pin Tab`→`Maak tabblad vast`) · `high`. Dus `Pin to switcher` →
  **`Maak vast in volumekiezer`**: Apples eigen `Maak vast in <plek>`-vorm met de al vastgelegde naam `volumekiezer`.
  Het partikel staat hier vóór de plaatsbepaling omdat Apple dat zelf zo schrijft; de partikel-achteraan-regel in
  `style.md` gaat over `Voeg … toe`, waar de voorzetselgroep een complement van het werkwoord is.
- **Check Again → `Controleer opnieuw`** · Mail `ConnectionDoctor.loctable`, SoftwareUpdate
  `SUSoftwareUpdateController.loctable`, VoiceBankingUI · `high`. Dus `Re-check` → `Controleer opnieuw`: het Engels is
  ander woordgebruik voor precies dezelfde knop.
- **Not found → `Niet gevonden`** · macOS `Localizable.loctable` (`Not found`→`Niet gevonden`, `Not Found`→
  `Niet gevonden`, `Not found in keychain`→`Niet gevonden in sleutelhanger`) · `high`.
- **Forget (knop) → `Vergeet`** · macOS `Localizable.loctable` (`Forget`→`Vergeet`, `Forget This Device`→
  `Vergeet dit apparaat`, `Forget This Network`→`Vergeet dit netwerk`) · `high`. Gelijk aan `menu.network.forgetServer`
  (`Vergeet server`), zoals de `@key` vraagt.
- **Trusted <ding> → `Vertrouwde <ding>`** · macOS `Localizable.loctable` (`Trusted servers`→`Vertrouwde servers`,
  `Trusted certificate`→`Vertrouwd certificaat`, `Trusted Locations`→`Vertrouwde locaties`) · `high`.
- **Choose the <ding> → `Kies de/het <ding>`** · macOS `Localizable.loctable`, tientallen zinnen
  (`Choose the volume you would like to restore to '%@'.`→`Kies het volume dat je wilt terugzetten op '%@'.`) · `high`.
- **Browse → `Blader`; Browse <plek> → `Blader op <plek>`** · Finder `nl.lproj/ConnectToWindow.strings` `48.title` en
  macOS `Localizable.loctable` (`Browse %@`→`Blader op %@`) · `high`. `Browse…` blijft `Blader…`, gelijk aan
  `servers.sheet.browse`.
- **in de gaten houden** als Apples weergave van doorlopend meekijken (`Keep track of your everyday items`→
  `Houd je veelgebruikte objecten in de gaten`) · `high` op het idioom, maar niet gebruikt: zie `Cmdr let op telefoons.`
  in `review-queue.md`.

Vormen die uit de eigen catalogus kwamen (byte-identiek Engels wint van een frisse keuze):

- `Got it` → **`Begrepen`**, byte-identiek aan `ai.toast.gotIt`, `main.oldMacos.gotIt` en
  `updates.moveToApplicationsDialog.gotIt`.
- `Right-click <ding>` → **`Klik met de rechtermuisknop op <ding>`** (`errors.listing.*.suggestion`,
  `fileExplorer.navigation.favoriteTooltip`); `command palette` → **`opdrachtenpalet`** (`menu.view.commandPalette`,
  `shortcuts.scope.commandPalette`), met `in het opdrachtenpalet` zoals `main.upgradeNudge.other`.
- `Favorites` (de groep in de volumekiezer) → **`Favorieten`** (`fileExplorer.navigation.groupFavorites`); de groep
  `Network` heet **`Netwerk`** (`fileExplorer.navigation.groupNetwork`), dus de meldingskop noemt hem
  `de groep Netwerk`.
- `USB debugging` → **`USB-foutopsporing`** en `turned on` → **`ingeschakeld`**, gelijk aan
  `settings.fileOperations.adbEnabled.description`; `Android platform tools` blijft Engels, gelijk aan diezelfde
  sleutel; `de opdracht adb` volgt `settings.fileOperations.adbBinaryPath.*`.
- `Look for adb the usual way` → **`Zoek op de gebruikelijke manier naar adb`**, dezelfde woorden als
  `settings.fileOperations.adbBinaryPath.description` (`dan zoekt Cmdr op de gebruikelijke manier naar adb`).
- Het interne hint-paar volgt `settings.behavior.openTerminalHereToastSeen.*` en `.doubleClickOnPaneNotificationSeen.*`:
  `Hint voor … getoond` / `Of de eenmalige hint over … is getoond.`
- Het tintlabel volgt zijn twee zusjes (`tintLocal`, `tintMtp`): `<soort>panelen tinten` plus
  `Achtergrondtint voor panelen die … tonen.`, met de opsomming zonder komma vóór `of`, zoals `tintMtp.description`.

Nieuw gemunte vormen, zonder bron in een bundel:

- **host key → `serversleutel`** · `tentative`. Het oude rijtje `host key → de sleutel` blijft gelden waar het Engels
  alleen `key` zegt (het serverpaneel, `hostKey.*`); hier zégt het Engels `host key`, en een instellingenpagina met een
  lijst heeft een zelfstandig naamwoord nodig. `hostsleutel` was al afgewezen; `serversleutel` sluit aan bij de
  paginanaam `Servers (SFTP, WebDAV)` en bij Apples eigen bezitsvorm (`de sleutel van de host`,
  `Vertrouwde certificaten voor server`). Dus `Trusted host keys` → `Vertrouwde serversleutels` en
  `The SSH host keys you have trusted.` → `De SSH-serversleutels die je hebt vertrouwd.`
- **Trusted <datum> → `Vertrouwd op`** · `high` op `Vertrouwd`, `tentative` op het voorzetsel. `Vertrouwd 2026-09-07`
  bestaat niet in het Nederlands, en `DateLabel` zet er een absolute datum achter (geen `vandaag`), dus `op` kan er
  veilig bij.
- **Found at {path} → `Gevonden: {path}`** · `tentative`. Apple heeft geen `Found at`; wel `Location: %@` →
  `Locatie: %@`, en die dubbelepunt-vorm ontwijkt de vraag of een pad `op` of `in` krijgt (`in` hoort bij een map, en
  `{path}` eindigt op het programma zelf). Werkt ook bij een lang pad dat erachter afbreekt.
- **Watching for phones. → `Cmdr let op telefoons.`** · `tentative`, zie `review-queue.md`.

Notities:

- **`menu.network.*` is een RAW-familie**: gewone apostroffen, geen ICU-verdubbeling. Geen van beide waarden bevat er
  een.
- **Geen enkele ICU-waarde in deze pass bevat een ASCII-apostrof**, dus nergens verdubbeling. De aanhalingstekens in
  `servers.pinHint.body` en `settings.adb.install.intro` zijn de enkele krulletjes `‘…’` van macOS-Nederlands
  (U+2018/U+2019, geen ICU-escapeteken). Het Engels zet `Unpin` en `Re-check` zónder aanhalingstekens in de zin; het
  Nederlands zet ze er wél omheen, zoals Finder dat doet (`Klik op ‘Ga door’`), en de labels staan er woord voor woord
  in: `Maak los` en `Controleer opnieuw`.
- **`{command}`, `{path}` en `{host}` zijn ongecontroleerde invoegingen** en krijgen geen voornaamwoord.
  `trustedHostKeys.confirm` zegt daarom `de vingerafdruk van de sleutel` in plaats van `zijn vingerafdruk`, en
  `servers.pinHint.body` sluit af met `De server blijft in de lijst Servers staan.` in plaats van `Hij blijft …`.
- **`vraagt het of je die vertrouwt`**: het Engels laat `and asks` zonder object staan; het Nederlands kan dat niet, dus
  de vraag wordt uitgeschreven. `het` is Cmdr, zoals in `settings.behavior.openTerminalHereApp.description`
  (`de terminal-apps die het op deze Mac vindt`).
- **`Deze sleutel vergeten?`** staat in de infinitief-eindvorm van een bevestigingsvraag
  (`fileExplorer.network.browser.removeHostConfirm`), wat Apple ook doet (`Forget %lld Wi‑Fi networks?` →
  `%lld WLAN-netwerken vergeten?`), met de knop ernaast in de imperatief (`Vergeet`).
- **`sameAsSourceJustification` staat op drie sleutels**: `settings.section.servers` (`Servers` is ook het Nederlandse
  meervoud, en `SFTP`/`WebDAV` zijn protocolnamen), `settings.section.adb` (`Android` is een productnaam, `ADB` de
  afkorting) en `settings.adb.status.label` (`Status`→`Status`, dezelfde bron als `servers.hub.colStatus`).

## Het telefoonpaneel via ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

Negentien sleutels: het volledige paneel dat in de plaats van een bestandslijst komt zodra het openen van een
Android-telefoon strandt, de zweeftips op een telefoonrij in de volumekiezer, de stille regel bovenaan een
MTP-telefoonpaneel, en het interne zie-vlaggetje van die regel.

De referentiestapel (`_ignored/i18n/nl/`) ontbreekt op deze machine (de M1-agentbox), dus Tier 1 komt uit de LIVE
macOS-bundels volgens `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?", alles geverifieerd op
macOS 26.6.2, build 25G83, 2026-09-07. Voor de twee Android-termen is de leverancier zélf de bron: AOSP's eigen
Nederlandse `values-nl/strings.xml`, opgehaald op 2026-09-07.

Termen uit Androids eigen Nederlands (`android.googlesource.com`, branch `main`):

- **USB debugging → `USB-foutopsporing`** · `frameworks/base/packages/SettingsLib/res/values-nl/strings.xml`,
  `enable_adb` = `USB-foutopsporing` (en `enable_adb_summary` = `Foutopsporingsmodus bij USB-verbinding`) · `high`. Dit
  tilt de eerdere `tentative`-keuze uit de ADB-instellingenpass naar Tier-1: dit is letterlijk het label dat op een
  Nederlandse telefoon onder Ontwikkelaarsopties staat, dus de gebruiker vindt de schakelaar op de woorden die Cmdr
  gebruikt. De Engelse `@key.description` vraagt om Engels te houden; de leverancier wint, zoals bij Apple
  (term-keuzeprincipe 1). Zie `review-queue.md`.
- **Allow (de knop op Androids eigen dialoog) → `Toestaan`** · `frameworks/base/packages/SystemUI/res/values-nl/`,
  `usb_debugging_allow` = `Toestaan`, in de dialoog `usb_debugging_title` = `USB-foutopsporing toestaan?` · `high`.
  Daarom `Kijk op je telefoon en tik op ‘Toestaan’.`: het woord in Cmdr is het woord op het scherm van de telefoon.
- **tap → `tik op`** · AOSP `packages/apps/Settings/res/values-nl/` (`Tik op een melding`, `Tik op de zwevende knop`) ·
  `high`. Androids werkwoord, niet dat van Apple, want de handeling gebeurt op de telefoon.
- **Turn on <ding> → `Zet <ding> aan`** · AOSP `SettingsLib` `adb_wireless_list_empty_off`
  (`Zet draadloze foutopsporing aan om beschikbare apparaten te bekijken`) · `high`. Dus `Turn on USB debugging.` →
  `Zet USB-foutopsporing aan.` Het gevestigde `turned on → ingeschakeld` blijft voor het instellingenlabel zelf; een
  aansporing in een hint gebruikt Androids eigen imperatief.
- **wake its screen → `zet het scherm aan`** · AOSP `SettingsLib` `allow_turn_screen_on_description`
  (`Sta toe dat een app het scherm aanzet`) · `high`. Apples eigen idioom is `uit de sluimerstand halen`
  (`PhotosUICore.loctable`), maar dat is een Mac-zin van drie woorden lang voor een zweeftip; Androids
  `het scherm aanzetten` beschrijft precies de handeling op de telefoon.

Termen uit de live macOS-bundels:

- **Open Settings → `Open Instellingen`** · letterlijk deze waarde in acht bundels tegelijk (FaceTime `General`,
  ActionKit, SiriSettingsUI, AuthKit, GameStoreKit, WorkflowUI, AppStoreDaemon, SensitiveContentAnalysisUI, telkens
  `Localizable.loctable`) · `high`. Hoofdletter op `Instellingen`, want het is de naam van het venster.
- **is not responding → `reageert niet`** · `loginwindow.loctable` (`An app failed to quit and is not responding.` →
  `Een app is niet gestopt en reageert niet.`) · `high`. Sluit aan op het al gevestigde `Waiting for X to respond` →
  `Wachten tot X reageert`.
- **the connection … was lost → `de verbinding … is verbroken`** · macOS `URL.loctable` (`Lost connection to host %@` →
  `De verbinding met host '%@' is verbroken.`) en de synchronisatiefamilie
  (`… omdat de verbinding met de iPhone is verbroken`) · `high`. Nederlands zet dit passief; het Engels heeft Cmdr als
  onderwerp. Omdat een weggelaten merknaam door `desktop-i18n-dont-translate` wordt gemeld, blijft `Cmdr` als deelnemer
  in de zin staan: `De verbinding tussen Cmdr en je telefoon is verbroken.`
- **reseat the cable → `sluit de kabel opnieuw aan`** · macOS AirPort
  (`Koppel de USB-kabel los van je %1$@ … en sluit hem aan op je %3$@`) plus de catalogus
  (`errors.listing.deviceProblem.suggestion`, `Koppel het apparaat los en sluit het opnieuw aan`) · `high`.

Vormen die uit de eigen catalogus kwamen (byte-identiek of parallel Engels wint van een frisse keuze):

- `Dismiss` → **`Sluit`**, gelijk aan alle tien de bestaande `Dismiss`-sleutels (`queue.row.dismiss`,
  `crashReporter.dialog.dismiss`, `lowDiskSpace.toast.closeTooltip`, …). Ook een rij in een lijst wordt gesloten, nooit
  gewist (`Wis` betekent uitvegen).
- `Disconnect {name}` → **`Verbreek de verbinding met {name}`**, byte-identiek aan
  `fileExplorer.navigation.disconnectPlaceAriaLabel` voor een server, zoals de opdracht vraagt. Het is bewust géén
  `Werp {name} uit`: er wordt niets veilig om los te koppelen gemaakt, de telefoon blijft aan de kabel.
- `Can''t disconnect while operations are in progress on this device` →
  **`Verbinding verbreken kan niet terwijl er bewerkingen op dit apparaat bezig zijn`**: de zinsbouw van
  `fileExplorer.navigation.disconnectBusyTooltip` (server) met het zelfstandig naamwoord van
  `fileExplorer.navigation.ejectBusyTooltip` (`dit apparaat`), precies zoals het Engels de twee combineert.
- `{host} reageerde niet op tijd.` (`servers.refusal.timedOut`) levert **`Je telefoon reageerde niet op tijd.`**
- `bladeren op een Android-telefoon` (`settings.summary.adb`) levert **`Cmdr kan er niet op bladeren`**.
- `Android platform tools` blijft Engels en `Android tooling` → `Android-tools`, gelijk aan
  `settings.fileOperations.adbEnabled.description` en `settings.adb.install.intro`.
- Het interne hint-paar volgt `settings.behavior.serversPinHintSeen.*`: `Hint voor … gesloten` /
  `Of de eenmalige regel over … is gesloten.` (`gesloten` in plaats van `getoond`, want het Engels zegt `dismissed`).

Nieuw gemunte vormen, zonder bron in een bundel:

- **`aangesloten` voor een telefoon aan een kabel** (`adb.connect.deviceGone`, `Je telefoon is niet meer aangesloten.`)
  · `high` op het woord, `tentative` op de keuze hier. Het onderscheid is al vastgelegd in de catalogus:
  `errors.listing.deviceReconnecting.explanation` zegt `Het apparaat is nog steeds aangesloten` over de fysieke kabel,
  terwijl `verbonden` de logische verbinding is. Deze sleutel gaat over de stekker, dus `aangesloten`.
- **`Hoe?`** (`adb.hint.how`) · `tentative`. Het Engelse `How` is één woord zonder vraagteken; het kale Nederlandse
  `Hoe` leest als een afgekapte zin, dus het vraagteken maakt er de vraag van die de `@key` beschrijft ("how do I do
  that?"). Geen bron: geen bundel heeft een linklabel van dit formaat.
- **`Zodra je dat doet, opent Cmdr je telefoon.`** (`adb.connect.waitingHint`) · `tentative`. Het Engelse
  `as soon as you do` leunt op het werkwoord van de vorige zin (`tap Allow`); Nederlands kan dat niet met één woord
  overnemen, dus `dat doet` verwijst terug naar de handeling. De bijzin staat vooraan omdat de geruststelling ("er hoeft
  niets meer") dan het eerst leest.

Notities:

- **Geen enkele Nederlandse waarde in deze pass bevat een ASCII-apostrof**, dus nergens ICU-verdubbeling, ook al doen
  bijna alle Engelse waarden dat wel (`couldn''t`, `isn''t`, `didn''t`). De aanhalingstekens rond `‘Toestaan’` zijn de
  enkele krulletjes van macOS-Nederlands (U+2018/U+2019), geen ICU-escapeteken.
- **`adb.disconnectDeviceAriaLabel` is geen `*Aria`-paar in de WCAG-zin**: er is geen zichtbaar label naast (de knop is
  een icoon), en het Engelse `fooAria` heeft geen `foo`-zusje, dus `desktop-i18n-aria-label` kijkt er niet naar. De
  waarde is toch de volle zin, want hij dient óók als zweeftip.
- **`adb.volumeLabelWithSuffix` was al vertaald** (`{deviceName} (ADB)`, met een `sameAsSourceJustification`) en is
  ongemoeid gelaten.
- **Nooit een diagnose noemen**: geen `adb-server`, geen `transport`, geen `daemon`, geen serienummer.
  `adb.connect.serverUnreachable` zegt daarom `De Android-tools op deze Mac reageerden niet.` en noemt het
  achtergrondprogramma niet.
- **`You stopped opening your phone.` → `Je hebt het openen van je telefoon gestopt.`** (`adb.connect.cancelled`) ·
  `stoppen`, net als de parallelle sleutel `search.coverage.walk.cancelled` (`Je hebt deze zoekopdracht gestopt`) en
  `errors.volume.cancelled` (`Cmdr heeft dit op je verzoek gestopt.`) · `high`. ❌ Niet `geannuleerd`: `Annuleer` is het
  LABEL van de knop (`fileOperations.button.cancel`), en de zin zou dan naar die knop lijken te wijzen.
  `het openen van …` staat al zo in de catalogus, en `openen` is het gevestigde werkwoord voor een telefoon
  (`adb.connect.waitingHint`, `Zodra je dat doet, opent Cmdr je telefoon.`).

## De vastgezette serveridentiteit (`servers.sheet.identityLocked`)

De twee regels onder de grijze velden `Adres` en `Gebruikersnaam`, wanneer de gebruiker een bewaarde server WIJZIGT.

- **`the account` (het veld waarmee je op de server inlogt) → `het account`** · de catalogus gebruikt het al in precies
  die zin (vijf keer in `errors.json`, één keer in `onboarding.json`) · `high`.
- **De hint noemt de acties precies zoals de knoppen waar hij naar verwijst**: `vergeten` uit
  `menu.network.forgetServer` ("Vergeet server") en `toevoegen` uit `servers.sheet.addTitle` ("Voeg server toe"). Een
  synoniem ("verwijder", "maak aan") stuurt de lezer op zoek naar een menu dat er niet is. Het scheidbare werkwoord
  houdt zijn partikel aan het EIND: `voeg hem opnieuw toe`, niet "voeg opnieuw toe hem".
- **`are what name this server` → `bepalen welke server dit is`** · het formulier heeft een eigen veld `Naam`
  (`servers.sheet.name`), dus de zin mag niet op "noemen" leunen: dan lijkt het over dat label te gaan. De welke-vraag
  zegt wat bedoeld is (die twee waarden ZIJN de server) en klinkt Nederlandser dan het Latijnse "identificeren" ·
  `high`.

## De toast als er helemaal geen wachtwoord opgeslagen was (`fileExplorer.navigation.forgetSecretNoneToast`)

- **`There was no saved password for {name}.` → `Er was geen opgeslagen wachtwoord voor {name}.`** · neemt
  `opgeslagen wachtwoord` en `voor {name}` woordelijk over van de drie geleverde zusjes
  (`menu.network.forgetSavedPassword` en `fileExplorer.navigation.forgetSecretConfirmTitle` =
  `Vergeet opgeslagen wachtwoord`, `.forgetSecretConfirm`, `.forgetSecretRefusedToast`) · `high`.
- **`Er was …` is de Nederlandse bestaanszin in de verleden tijd**, dezelfde vorm die de catalogus al gebruikt
  (`fileOperations.trash.undoUnavailable`: `Er is niets terug te zetten.`). Geen `fout` en geen `mislukt`: er ging niets
  mis, dezelfde toonregel als bij `forgetSecretRefusedToast`.
- De referentieverzameling stond niet op deze machine (`_ignored/i18n/` ontbreekt ook in de hoofdclone), dus de keuze
  leunt op de al geleverde catalogus en op `terms.json`.

## De herhaalduur, de kop over de hostsleutel en Androids knop Toestaan

- **`{seconds}`/`{minutes}` hebben nu een ICU-meervoud met TWEE plaatsaanduidingen**
  (`servers.paneState.retryTotalSeconds`, `.retryTotalMinutes`): `{seconds}` kiest alleen de tak, wat de lezer ziet is
  `{secondsText}`, het al opgemaakte getal. Nederlands heeft `one` en `other` (CLDR, § style.md): `1 seconde` /
  `60 seconden`, `1 minuut` / `2 minuten`, woordelijk uit `main.quit.countdown` en `indexing.eta.hoursMinutesLeft` ·
  `high`.
- **Beide waarden zijn bouwstenen van `servers.paneState.retryKeepsTrying`**
  (`We blijven het in totaal {duration} proberen.`), dus ze blijven kaal: geen voorzetsel, geen punt.
- **`Cmdr won't connect to {name}` → `Cmdr maakt geen verbinding met {name}`** · dezelfde constructie als het zusje
  `servers.refusal.hostKeyRevoked` (`Cmdr maakt er geen verbinding mee.`). Het Engels ging van „stopped connecting” naar
  een blijvende weigering, dus geen `niet meer`: dat suggereert dat het eerder wél lukte · `high`.
- **`Allow` is Androids eigen knop → `‘Toestaan’`**, woordelijk uit `adb.connect.unauthorized`
  (`Kijk op je telefoon en tik op ‘Toestaan’.`), inclusief de enkele aanhalingstekens en het werkwoord `tikken op`,
  zodat de zin en het scherm hetzelfde woord tonen · `high`.
- De referentieverzameling stond niet op deze machine (`_ignored/i18n/` ontbreekt ook in de hoofdclone), dus de keuze
  leunt op de al geleverde catalogus en op `terms.json`.

## Het contextmenu van de serverrij: `Open` en `Wijzig server…`

- **`Open` (op een serverrij) → `Open`** (`menu.network.open`) · woordelijk gelijk aan `menu.file.open`, want het is
  dezelfde betekenis: ergens naar binnen gaan, niet een bestand aan een app geven. Het Nederlands scheidt die twee niet,
  en macOS ook niet: de Finder gebruikt hetzelfde werkwoord in `Open` (`LocalizableMerged` `N151`), `Open met` (`N152`)
  en `Open in nieuw venster` (`FV7`, het naar-binnen-gaan) (Finder 26.6.2, build 25G83, gelezen op 2026-09-07) · `high`.
  De kale stam-imperatief valt toevallig samen met het Engelse woord, dus de sleutel draagt een
  `sameAsSourceJustification`, net als `menu.file.open`.
- **`Edit server…` → `Wijzig server…`** (`menu.network.edit`), byte voor byte overgenomen uit
  `commands.serversEdit.label` · `high`. Beide openen hetzelfde blad; twee labels zouden als twee functies lezen. De
  puntjes zijn het ENE teken `…` (U+2026) en blijven staan. De stam-imperatief past ook bij de buren (`Verbreek`,
  `Vergeet server`).
- **Beide gelijkheden worden bewaakt**, ze zijn niet alleen netjes: `i18n-terms` meldt het als twee sleutels met
  dezelfde Engelse waarde in het Nederlands uit elkaar lopen. Herschrijf je er één, dan moet de partner mee.
- **`menu.*` is een RAW-familie**: Rust tekent het menu via `menu_t`, nooit via `t()`. Apostrofs blijven dus ENKEL, een
  verdubbelde `''` laat `i18n-icu` falen. In deze twee waarden staat er geen.
- De referentieverzameling ontbreekt op deze machine, maar `Finder.app` levert hetzelfde Tier-1-bewijs rechtstreeks uit
  het systeem (`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?").

## Function key bar context menu

- function key bar (de rij met functietoets-commandoknoppen onderin het venster) → functietoetsbalk · al vastgelegd in
  de catalogus (`settings.appearance.showFunctionKeyBar.label`); hergebruikt voor het contextmenu-item en de
  bijbehorende toast · high

## Het Dock-aanbod (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- Dock → `Dock`, onvertaald, MET het lidwoord `het` in lopende tekst · macOS Finder (`Voeg toe aan Dock`,
  `LocalizableMerged` `N169.13` en `MenuBar` `300772.title`), `Dock.app` `nl.lproj/DockMenus.strings`
  (`REMOVE_FROM_DOCK` = `Verwijder uit Dock`, `DOCK_SETTINGS` = `Dock-instellingen…`) en Systeeminstellingen
  (`DesktopSettings.appex` `Localizable.loctable`, `Automatically hide and show the Dock` →
  `Toon/verberg **het** Dock automatisch`). De catalogus zegt het al zo (`errors.listing.storageFull.suggestion`:
  `het Prullenmand-symbool in het Dock`). Apple laat het lidwoord weg in korte labels, maar niet in zinnen · `high`
  (macOS 26.6.2, build 25G83, gelezen 2026-09-09)
- Finder → `Finder`, onvertaald · al vastgelegd in § Native menu's; macOS Dutch zegt `Verberg Finder`, `Toon in Finder`
  · `high`
- Applications (de map met geïnstalleerde apps) → `de map ‘Apps’` · macOS Finder heet die map in het Nederlands `Apps`,
  niet `Programma's`: `LocalizableMerged` `TL5` / `GROUP_APPLICATIONS` = `Apps`, `MenuBar` `258.title` (het Ga-menu) =
  `Apps`, en de knopbaltip `TL_HELP_APPS` = `Ga naar de map 'Apps'`, precies deze constructie · `high`. `Programma's`
  komt in het Nederlandse macOS alleen nog voor in `Hulpprogramma's` (Utilities)
- configuration profile → `configuratieprofiel` · Systeeminstellingen `InfoPlist.json` (`Configuratieprofiel`) plus
  Microsoft-terminologie (`configuratieprofiel`); twee tiers zijn het eens · `high`
- pin / unpin (in het Dock) → `vastzetten` / `losmaken` · het paar dat de catalogus overal gebruikt (`menu.tab.pinTab` =
  `Maak tabblad vast`, `menu.tab.unpinTab` = `Maak tabblad los`, `menu.network.pinToSwitcher`,
  `commands.serversTogglePin.label`), zodat `vastzetten` in de body en `losmaken` in de losmaakregel als één paar lezen
  · `high`. Apples eigen Dock-menu zegt `Permanent in Dock` (`KEEP_IN_DOCK`) en `Verwijder uit Dock`; dat zijn labels
  van menu-items, geen werkwoordpaar, dus ze winnen hier niet van de catalogusconsistentie
- icon → `symbool` · macOS Finder (`symboolweergave`) en de catalogus zelf
  (`settings.appearance.useAppIconsForDocuments.description`: `het symbool van de app`) · `high`
- log in (bij je Mac) → `inloggen` · `Dock.app` `DockMenus.strings` `OPEN_AT_LOGIN` = `Open na inloggen`; sluit aan op
  de al vastgelegde `Log in` · `high`
- add to the Dock (knop) → `Voeg toe aan Dock` · macOS Finder woordelijk. ❗ Let op: Apple zet het partikel hier NIET
  achteraan, tegen de algemene regel in § Formality mechanics in; een woordelijk Tier-1-citaat voor precies dit label
  wint van de algemene regel. `main.dockPinNudge.accept` = `Ja, voeg toe aan mijn Dock` · `high`

Toonkeuzes in deze negen strings:

- **`een paar dagen`, nooit een getal.** Het Engels houdt de drempel expres vaag; `een paar dagen` doet hetzelfde.
- **Geen enkele uitkomststring noemt `fout` of `mislukt`.** `addedButDockDidNotRestart` zegt dat het symbool al
  klaarstaat en alleen het Dock nog niet opnieuw geladen is (`nog niet` in plaats van `niet`, zodat het als tijdelijk
  leest en niet als weigering); `notAdded` zegt `is deze keer niet in het Dock terechtgekomen` en wijst meteen de
  handmatige weg; `managedDock` noemt de beheerder zonder verwijt.
- **`Wie deze Mac beheert, kan dat aanpassen.`** houdt de zin geslachtsneutraal zonder typografische trucs: het Engelse
  `whoever` wordt een vrije relatieve bijzin, geen `hij/zij`.
- **`erin slepen`** (`notAdded`) vermijdt dat `het Dock` twee keer in twee zinnen staat; het voornaamwoordelijk bijwoord
  werkt hier omdat het Dock in de vorige zin genoemd is.

## Het Dock-menu van Cmdr zelf (`menu.dock.*`)

De vijf items die verschijnen als je met rechts op Cmdrs symbool in het Dock klikt. RAW-familie (`menu.*`), dus enkele
apostroffen en letterlijke `{name}` / `{parent}`. Twee items hebben een exacte Finder-tegenhanger, en die wint: de
gebruiker ziet Cmdrs Dock-menu naast dat van Finder. Gemijnd in `_ignored/i18n/nl/macOS/Finder/MenuBar.json` en, voor
het Dock zelf, in de live bundels (macOS 26.6.2, build 25G83, gelezen 2026-09-09).

- Open Cmdr → `Open Cmdr` (bewust gelijk aan het Engels) · `Dock.app` `nl.lproj/DockMenus.strings`: `OPEN` = `Open`,
  `OPEN_FILENAME` = `Open '%@'`. Precies dit item, in precies dit menu, heet in het Nederlandse Dock `Open`; de
  stam-imperatief van `openen` valt samen met het Engelse woord, en `Cmdr` blijft onvertaald · `high`
- Go to Folder… → `Ga naar map…` · macOS Finder `MenuBar` `261.title`, woordelijk (`en_GB` = `Go to Folder…`) · `high`.
  Onderscheiden van de bestaande `menu.go.goToPath` = `Ga naar pad…`: die vraagt om een pad, dit item is Finders eigen
  label
- Connect to Server… → `Verbind met server…` · macOS Finder `MenuBar` `266.title`, woordelijk (`en_GB` =
  `Connect to Server…`), en al zo in de catalogus (`commands.serversConnect.label`,
  `settings.network.permissionIntroConnectLink`) · `high`
- Search files… → `Zoek bestanden…` · byte-identiek aan `menu.edit.searchFiles`, dezelfde opdracht in de menubalk
  (dezelfde `sourceHash`), dus dezelfde waarde · `high`
- `{name} ({parent})` → `{name} ({parent})` (bewust gelijk aan het Engels) · twee mapnamen en een haakje, geen te
  vertalen woord. Het Nederlands zet de verduidelijking net zo achter de kern, en Apple houdt het patroon zelf
  ongewijzigd (Finder `LocalizableMerged` `SB_iCloudDetail` = `^0 (^1)`, AppKit `Common`/`InfoPanel` `%@ (%@)` =
  `%1$@ (%2$@)`), net als de catalogus in `menu.volume.eject` en `menu.context.openWithDefault` · `high`

Merk op dat Apples Dock-menu voor `eject` `Verwijder` zegt (`DockMenus.strings` `EJECT`); dat bevestigt de al
vastgelegde uitzondering in § Native menu's, waar Cmdr `Werp uit` gebruikt om de botsing met _delete_ te vermijden.

## Het ‘Toon in Finder’-aanbod en de melding bij de eerste keer (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

Twee momenten van dezelfde functie: het eenmalige aanbod om ‘Toon in Finder’ uit andere apps in Cmdr te openen, en de
eenmalige melding wanneer zo'n verzoek hier voor het eerst binnenkomt. Beide oppervlakken wijzen naar macOS' eigen
opdrachten, dus de macOS-terminologie wint (style.md § systeemoppervlakken).

- **“Show in Finder” → `‘Toon in Finder’`, met enkele typografische aanhalingstekens** · Al vastgelegd in
  `settings.navigationAndFileOps.card.showInFinder` en `settings.revealHandler.description` · `high`. De toasts nemen
  precies die vorm over, zodat de instellingenkaart en de melding dezelfde actie hetzelfde noemen.
- **pane → `paneel`** · Catalogusvorm in `fileExplorer.doubleClickHint.body` (‘paneelachtergrond’) · `high`.
- **Settings (Cmdrs eigen venster) → `Instellingen`, met hoofdletter** · `settings.window.title` · `high`.
- **“for a while now” → `al een tijdje`** · Bewust vaag: de drempel kan verschuiven, dus ❌ nooit een getal. Zelfde
  regel als `main.dockPinNudge.body` (‘een paar dagen’) · `high`.
- **De melding bij de eerste keer is ❌ geen excuus** · Ze zegt wat er net gebeurde, waarom, en waar de schakelaar
  staat. Vandaar `Cmdr staat ingesteld om deze op te vangen`, ❌ nooit ‘sorry’ · `high`.

## De onboarding-herschrijving: de checklist, de stapteller en de vier verdictregels

23 sleutels uit `onboarding.json`: stap 1 en het wizardframe, de AI-stap, de checklist van stap 3, de vier
samenvattingen van stap 4, plus vier sleutels waarvan het Engels herschreven werd. Gemijnd in `_ignored/i18n/nl/`
(macOS, Microsoft-terminologie, de vijf bestandsbeheerders) en, waar de stapel zwijgt, in de live macOS-bundels (macOS
26.6.2, build 25G83, gelezen 2026-09-09).

### GitHub en AlternativeTo: `star` en `like` blijven Engels, maar krijgen een Nederlands werkwoord

- **star (een repository markeren op GitHub) → `star` (zelfstandig naamwoord), knoptekst `Geef de repo een star`** ·
  Microsoft-terminologie `DUTCH.tbx`, precies deze betekenis ("A bookmark or display of appreciation for a repository.
  Stars are a manual way to rank the popularity of projects."), NLD + BEL, Nederlandse term = `star` (onvertaald) ·
  `high`. GitHub zelf levert geen Nederlands: de meertalige interface is op 2016-11-18 stopgezet, dus een Nederlandse
  gebruiker ziet letterlijk de knop `Star`. Daarom blijft het woord staan en draagt het Nederlands alleen het werkwoord.
  ❌ Niet `ster` (dat is in `DUTCH.tbx` de beoordelings-ster) en niet `favoriet` (dat is de browser-bladwijzer).
  Meervoud `stars`, zoals in `checklist.starNote`.
- **like (een upvote op AlternativeTo) → `like` (zelfstandig naamwoord), knoptekst `Geef Cmdr een like`** ·
  AlternativeTo is een Engelstalige site, dus de knop die de gebruiker aanklikt heet `Like`; het Nederlandse
  `een like geven` is standaardtaal. Microsoft geeft voor de sociale betekenis `vind ik leuk` / `leuk vinden`
  (`DUTCH.tbx`), maar dat is Facebooks Nederlandse knop en die staat hier niet op het scherm · `high`
- **De twee rijen delen bewust één vorm**, `Geef <object> een <leenwoord> op <site>`. Wie er één herschrijft,
  herschrijft ze allebei: naast elkaar in dezelfde checklist leest een afwijkende tweede rij als een andere handeling.

### De checklist zelf

- **checklist → `checklist`** · Van Dale-leenwoord, ingeburgerd in Nederlandse interfaces. Microsoft zegt
  `controlelijst` (`DUTCH.tbx`), maar dat is de Planner/Kaizala-functie en klinkt naar inspectie, niet naar de vier
  kleine gunsten die deze lijst vraagt · `high`
- **`Onboardingchecklist`, aaneen** · `onboarding` blijft een leenwoord (zie boven) en de catalogus schrijft zulke
  samenstellingen al aaneen (`onboardingopties`). Lang woord, maar het is de correcte spelling; een streepje zou van de
  `onboardingopties`-lijn afwijken.
- **each takes 30 seconds → `elk punt kost 30 seconden`** · het Engelse `each` heeft geen antecedent dat het Nederlands
  kan overnemen, dus `punt` benoemt waar het over gaat · `high`
- **Save (de knop naast het e-mailveld) → `Bewaar`** · macOS AppKit (`Save` → `Bewaar`), stam-imperatief per de
  knopregel in `style.md` · `high`. De twee foutregels eronder noemen de knop met de enkele krulaanhalingstekens
  (`op ‘Bewaar’`), zodat de gebruiker het label letterlijk terugvindt.
- **Email address saved (de toegankelijke naam van het vinkje) → `E-mailadres bewaard`** · `e-mailadres` uit macOS
  (`Email Address` → `E-mailadres`) en Microsoft (`DUTCH.tbx`); een kort deelwoordelijk naamwoordgroepje, want de rij is
  een voortgangsmarkering, geen bedienbare knop · `high`
- **`<field></field>` staat achter het scheidbare partikel**: `Vul je e-mailadres in <field></field> om …`. Het vakje
  komt zo op dezelfde plek als in het Engels en de zin loopt door over het invoerveld heen. ❌ Zet het niet vóór `in`:
  dan valt het partikel van zijn werkwoord los.

### De twee inschrijfmeldingen

- **mailing list → `mailinglijst`** · standaardtaal (Van Dale) en de gangbare term voor een lijst waar je je zelf voor
  inschrijft. Microsofts `adressenlijst` (`DUTCH.tbx`) benoemt een ánder begrip, een distributielijst die een beheerder
  samenstelt, dus die term is hier niet van toepassing (mijngotcha 2) · `tentative`
- **signup server → `de inschrijfserver`** · gemunte samenstelling op het werkwoord dat de broer of zus
  `onboarding.stepBeta.signup.failure` al gebruikt (`Sorry, het inschrijven lukte nu niet`); `inschrijf-` is een normale
  Nederlandse samenstellingsstam (vgl. `inschrijfformulier`). Microsoft geeft `registreren` voor `sign up` en
  `aanmelden` voor `subscribe`, maar de catalogus heeft zijn eigen woord al gekozen en consistentie binnen het paneel
  weegt zwaarder · `tentative`
- **typo → `typefout`** · al in de catalogus (`licensing.error.badSignatureHint` "Controleer op typefouten") · `high`.
  De regel "geen `fout` als kaal label" gaat over foutmeldingen; `typefout` is de gewone naam van het ding.
- **Settings › Updates & privacy → `Instellingen › Updates en privacy`** · beide helften uit de catalogus zelf
  (`settings.section.updatesAndPrivacy` = `Updates en privacy`), en `›` blijft staan zoals in
  `settings.askCmdr.provider.shared` · `high`

### De vier verdictregels van stap 4

Elke regel staat naast een schakelaar en mag niet omlopen, dus ze blijven kort en volgen de terminologie van hun lange
`…desc`-tegenhanger.

- **"Local Network" → `‘Lokaal netwerk’`** · Apples eigen paneelrij, live gelezen (`Localizable.loctable`,
  `LOCAL_NETWORK`, macOS 26.6.2 build 25G83, 2026-09-09) · `high`. Zowel `stepOptional.networking.summary` als
  `…networking.desc` citeren die rij letterlijk; ❌ niet de omschrijving `Lokale netwerktoegang`, want die naam vindt de
  gebruiker niet terug in Systeeminstellingen.
- **folder sizes → `mapgroottes`** · al overal in de catalogus (`settings.indexing.enabled.description`,
  `indexing.step.findFilesChangeCheck`, `settings.section.fileAndFolderSizes`) · `high`
- **native handler (het macOS-proces dat MTP-apparaten inpikt) → `de macOS-afhandeling`** · `stepOptional.mtp.desc`
  noemt het `dat macOS-proces`; `afhandeling` vangt `handler` in één woord en houdt de regel even lang als het Engels ·
  `tentative`
- **one tiny check → `één piepkleine controle`** · `piepklein` staat al in `stepOptional.updates.desc` ("piepklein
  netwerkverzoek") · `high`

### De vier herschreven sleutels

- **`stepFda.ifAllow`**: "Three easy steps:" → `Drie eenvoudige stappen:`. Het Engels trivialiseert hier ("easy"), wat
  tegen `docs/style-guide.md` in gaat; de vertaling volgt de bron en de opmerking gaat naar boven, niet in de string.
- **`stepAi.cloud.help`**: `aangepast` voor `custom` komt uit macOS (`Custom` → `Aangepast`, `terms.json`); `ollama` en
  `LLM` blijven staan; `Configure it below` → `Stel het hieronder in`, met het partikel achteraan.
- **`stepAi.local.label`**: "super private" → **`superprivé`**. `privé` is het gewone Nederlandse woord en `super-` is
  een normaal versterkend voorvoegsel dat aaneen geschreven wordt (`supersnel`); ❌ niet het gemunte `supergeprivacyd` ·
  `high`. `simple local model` → `een eenvoudig lokaal model`: `eenvoudig` beschrijft hier het model, niet hoe makkelijk
  de gebruiker het heeft, dus de "geen simpel/makkelijk"-regel geldt niet.
- **`stepBeta.openBeta`**: de slotzin luidt `Jouw feedback helpt me bugs op te lossen en functies te prioriteren.` Er
  stond ooit een bijna identieke zin een paar regels lager, in de intro van de feedbackkanalenlijst; die lijst is met de
  herschrijving van stap 3 verdwenen, dus dit is nu de enige plek waar Cmdr dit zegt. `<alpha></alpha>-badges` houdt het
  streepje uit de al verzonden waarde, en `de meest in-ontwikkeling-zijnde gebieden` is vervangen door
  `de delen die het meest in ontwikkeling zijn`, dat het woord `delen` uit de eerste zin hergebruikt.

### De stapteller met de `+1`

- **`Stap {step} van {mandatory}+1`** · `onboarding.wizard.stepProgress` schrijft `Stap {step} van {total}` al zo, dus
  de tooltip erft die vorm; de literale `+1` blijft staan, want dat is de hele pointe van de formulering · `high`
- **optional → `optioneel` / `optionele`** · dezelfde sleutel `stepProgress` heeft `(optioneel)` al · `high`
- De `select`-takken zijn hele bijzinnen (`, er is nog één optionele stap` / `, dit is de laatste, optionele stap`), dus
  de komma van het Engels blijft binnen de tak staan.

### Uitgebrachte versie en testbuilds (`settings.revealHandler.notProductionBuild`)

- **released copy / Dev and test builds → `uitgebrachte versie` / `dev- en testbuilds`** · Microsoft terminology
  (`DUTCH.tbx`: `build` → `build`, `production build` → `productiebuild`, `uitbrengen` voor een product op de markt
  brengen), plus de catalogus (`buildmappen` in `search.systemDirExclude.default`, aaneengeschreven). `versie` en niet
  `kopie`: de zin gaat over een uitgave, zoals `commands.appCheckForUpdates.description`, niet over een tweede draaiend
  proces zoals `main.instanceLock.alertBody`. De tweede zin volgt `settings.revealHandler.notInApplications`
  (`… elke klik op ‘Toon in Finder’ naar niets laten wijzen`) · `high`.

## Het weergavevenster haalt een bestand eerst op (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- **fetch (een bestand van een telefoon, server of archief naar een tijdelijke kopie halen) → `ophalen`
  (`{fileName} ophalen om te bekijken`)** · macOS Finder `IN_MD1` (`Fetching…` → `Ophalen…`), Systeeminstellingen
  `Fetching Menu Item` (`Ophalen…`), Microsoft terminology (`DUTCH.tbx`, fetch → `ophalen`, NLD/BEL) · `high`. Een
  voortgangskop, dus infinitief met het object vooraan, zoals Apples `'%@' laden…`. `bekijken` volgt
  `viewer.error.tooLargeToPreview`; geen voornaamwoord naar `{fileName}`, en geen aanhalingstekens, zoals
  `downloads.notification.title` (`{fileName} gedownload`).
- **"{doneText} of {totalText}" → `{doneText} van {totalText}`** · Finder `PW8` (`^0 van ^1 gekopieerd`), Nautilus
  (`%s van %s`), de catalogus (`fileExplorer.diskSpace.free`) · `high`
- **"so far" (hoeveelheid zonder bekende totale grootte) → `{doneText} tot nu toe`** · de catalogus
  (`queryUi.results.live.matchesSoFar`, `search.imageResults.paused`) · `high`
- **"stopped arriving" (zo'n 45 seconden geen gegevens) → `Er komen geen gegevens van dit bestand meer binnen.`** · geen
  bron formuleert dit; bewust niet de stall-formule `komt niet meer vooruit`, want het weergavevenster toont geen
  overdracht · `tentative`. De tweede zin is wel belegd: `errors.provider.macDroid.transient`
  (`Controleer of je telefoon verbonden is`), `errors.listing.networkConnectionDropped.suggestion` (`nog verbonden is`),
  connected → `verbonden`, Finder `N178` (`Probeer het vervolgens opnieuw.`) · `high`

## De verouderde index van een telefoon via ADB (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

De telefoonversies van de drie zinnen over een verouderde schijfindex. Ze erven de termen van hun schijfzusjes, zodat de
twee families als één lezen, en zeggen nergens `losgekoppeld`: de telefoon zit nog aan de kabel.

- **phone → `telefoon`** · de hele `adb.*`-familie (`Je telefoon is niet meer aangesloten.`), plus
  `errors.listing.notConnected.*` en `errors.provider.macDroid.*` · `high`. Hier `deze telefoon` / `de telefoon`, want
  het Engels zegt `this phone` / `the phone`, niet `your phone`.
- **out of date (index) → `mogelijk verouderd`** · byte-parallel aan `indexing.staleDialog.title`
  (`De index van deze schijf is mogelijk verouderd`) · `high`
- **keeps the index current → `werkt de index bij`** · `bijwerken` is het gevestigde werkwoord voor de index
  (`fileExplorer.dirSize.updatingIndexTooltip` `Index wordt bijgewerkt`, `indexing.rescan.watcherStartFailed`
  `om de index bij te werken`) · `high`. ❌ Niet `houdt … bij`, dat in deze catalogus _registreren_ betekent
  (`settings.operationLog.intro` `Cmdr houdt je bestandsbewerkingen bij`).
- **the changes Cmdr makes itself → `zijn eigen wijzigingen` (zweeftip) / `de wijzigingen die Cmdr zelf aanbrengt`
  (dialoog)** · `zijn eigen` voor Cmdr staat al in `askCmdr.tool.memoryEdit.doing` (`Werkt zijn eigen notities bij`);
  `wijzigingen aanbrengen` in `errors.listing.readOnly.explanation` · `high`. In de dialoog staat `{name}` vóór Cmdr in
  de zin, dus `zijn` zou naar de telefoon kunnen wijzen en precies het omgekeerde zeggen; daar herhaalt de zin `Cmdr`,
  net als het Engels.
- **rescan → `opnieuw scannen` (zweeftip) / `opnieuw doorzoeken` (dialoog)** · Microsoft terminology (`DUTCH.tbx`,
  rescan → `opnieuw scannen`, NLD/BEL) plus `driveIndex.tooltipStale` (`Scan opnieuw`); de dialoog volgt
  `indexing.staleDialog.body` (`totdat je opnieuw doorzoekt`) · `high`. Elke zin houdt het werkwoord van zijn eigen
  zusje, dus `nadat je opnieuw scant` en `nadat je opnieuw doorzoekt`.
- **show up (in folder sizes and search) → `verschijnen in mapgroottes en zoekresultaten`** · `mapgroottes` en
  `zoekresultaten` uit `indexing.staleDialog.body` · `high` op de termen. `zoekresultaten` en niet het kale `zoeken`:
  `verschijnen in zoeken` is geen Nederlands.
- **its files → `de bestanden erop`** · `{name}` krijgt geen voornaamwoord (style.md), en `erop` staat al zo in
  `indexing.staleDialog.body` · `high`
- **stays as a reminder → `blijft zichtbaar als herinnering`** · geen bron in de stapel; gekozen boven
  `blijft als geheugensteuntje staan`, dat warmer maar langer is · `tentative`

## Hoofdmap en beginmap van een opgeslagen server (`servers.sheet.rootFolder*`, `servers.sheet.startFolder*`, `servers.refusal.*`)

Het blad voor een SFTP- of WebDAV-server heeft twee mapvelden: de map waar Cmdr nooit boven uitkomt, en de map waar een
paneel opent. Beide termen staan byte-gelijk in de labels, de hulpregels en de weigerzinnen.

- **root folder (het plafond op de server) → `hoofdmap`** · Microsoft terminology (`DUTCH.tbx`, `root folder` /
  `root directory` → `hoofdmap`, NLD/BEL, "The uppermost directory on a computer, partition or volume"), KDE Dolphin
  (`Search all directories from the root up` → `vanaf de hoofdmap (root)`) · `high`. macOS kent het begrip niet (geen
  `root folder` in Finder, AppKit of Systeeminstellingen). Afgewezen: Double Commanders `rootmap` (half Engels), Total
  Commanders `hoofddirectory` (verouderd, de catalogus zegt overal `map`), en het oude label `Externe map`, dat de
  plafondbetekenis niet draagt.
- **start folder (waar een paneel opent) → `beginmap`** · Double Commander (`Start path:` → `Beginpad:`,
  `Starting paths` → `Beginpaden`), met de gevestigde folder → `map` · `tentative`: geen bron heeft de samenstelling
  zelf. Gekozen boven `startmap`, dat aan Windows' `Opstartmap` en het Startmenu doet denken.
- **never goes above (this folder) → `gaat nooit hoger dan`** · geen bron; `bovenliggende map` staat al voor de map één
  niveau hoger, dus `hoger` sluit daar in beeld op aan · `tentative`
- **call (a server) by its account and host → `noemt … naar het account en de host`** · `het account` volgt
  `servers.sheet.identityLocked`; de zinsbouw `Laat dit leeg, dan … Cmdr` volgt
  `settings.fileOperations.adbBinaryPath.description` · `high` op de termen
- **can read it (je account) → `hem mag lezen`** · `mag` omdat de server toegang weigert (rechten), niet omdat het
  technisch niet kan; `hem` voor `de map`, zoals `Cmdr maakt hem aan` · `tentative`
- **nothing was saved → `er is niets bewaard`** · save → `bewaren` (macOS), `{host} reageerde niet op tijd` byte-gelijk
  aan `servers.refusal.timedOut` · `high`

## Waarom een gedeelde map niet aankoppelt of de lijst niet laadt (`errors.mount.*`, `errors.shareList.*`)

De zinnen onder „Gedeelde map aankoppelen lukte niet” (`fileExplorer.networkMount.mountFailedTitle`) en „Verbinden met
{hostName} lukte niet” (`fileExplorer.network.share.connectFailedTitle`), plus de meldingen
`fileExplorer.pane.directConnectionShareGoneToast`, `fileExplorer.pane.directConnectionMountNotRespondingToast`,
`fileExplorer.pane.directConnectionNotNetworkShareToast` en `servers.refusal.accountNotPermitted`. Tier 1 uit de LIVE
bundel `NetAuthAgent.app/Contents/Resources/Localizable.loctable` (macOS 26.6.2, 25G83, 2026-09-11), die precies deze
gevallen voor „Verbind met server” formuleert en niet in de stapel zit.

- **share (in het paneel) → `gedeelde map`** · de titel ernaast en `fileExplorer.network.share.notFound` · `high`.
  NetAuthAgent `EINFO_NO_SHARE` zegt `share`, maar het paneel heeft `gedeelde map` al vastgelegd. **network share →
  `netwerkshare`** blijft voor „is geen netwerkshare”, zoals `errors.eject.notAnSmbVolume`.
- **guests → `gasten`** · NetAuthAgent `EINFO_NO_ACCESS_GUEST` („Voor deze server is geen gasttoegang toegestaan.”) ·
  `high`
- **reach → `bereiken`**, **didn't answer in time → `reageerde niet op tijd`**, **isn't responding → `reageert niet`** ·
  `servers.refusal.unreachable`, `servers.refusal.timedOut`, de rij respond → `reageren` · `high`
- **turned on → `is ingeschakeld`** · `errors.listing.hostUnreachable.suggestion` · `high`
- **`{server}` en `{share}` krijgen geen voornaamwoord**: de zin herhaalt `de server` of `de gedeelde map` (`style.md`)
  · `high`
- **package → `pakket`** · KDE Dolphin („Kon pakket %1 niet vinden.”) · `high`. **distribution (Linux) → `distributie`**
  · geen bron in de Linux-betekenis · `tentative`
- **there's nothing to speed up → `dus er valt niets te versnellen`** · kader van `errors.eject.volumeNotFound` · `high`
- **Try again in a moment → `Probeer het zo meteen opnieuw.`** · `errors.volume.deletePending` · `high`
- Zelfde Engels, zelfde Nederlands: `errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` / `errors.shareList.authFailed`.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `teksteditor`** · Microsoft-terminologie (`text editor` → `teksteditor`; daarnaast `editor`) · `high`.
  De app-soort, ❌ niet Apples app TextEdit, waarvan de naam in `{app}` staat.
- **default text editor → `standaardeditor`** · de catalogus (`commands.fileEdit.label` "Bewerk in standaardeditor") ·
  `high`.
- **Edit files in [app] → `Bewerk bestanden in`** · `Bewerk` is in deze catalogus gereserveerd voor een bestand in een
  editor openen, precies dit · `high`.
- `{app}` na `in`, zonder lidwoord. Dismiss en Open settings gelijk aan `commands.handler.openTerminalHere.dismiss` /
  `commands.handler.openTerminalHere.openSettings`.
- **system default → `Systeemstandaard`** · de catalogus (`settings.appearance.language.opt.system`,
  `settings.appearance.dateTimeFormat.opt.system`) en de regel "system default → Systeemstandaard" bovenaan · `high`.
  `settings.behavior.textEditorApp.systemDefault` zet de appnaam erachter tussen haakjes, zoals
  `settings.appearance.language.opt.systemWithLanguage`.
- ‘Choose an app…’ en ‘Checking your apps…’ gelijk aan `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking`. De hint (`fileExplorer.edit.hint`) volgt
  `commands.handler.openTerminalHere.hint`, zonder de plek in Instellingen: de knop gaat er direct heen.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected (uitwerpen of ontkoppelen bezig) → `wordt losgekoppeld`** · de al vastgelegde vorm voor een
  schijf die tijdens een ronde wegvalt, en Thunar `nl` („Ontkoppelen van apparaat...”) zegt het lopende proces met
  dezelfde stam · `high`. „Left its index as it was” → „gelaten zoals die was”, zoals
  `operationLog.rollback.partiallyRolledBackNotice`; „try again in a moment” → „zo meteen opnieuw”, zoals
  `fileExplorer.pane.directConnectionMountNotRespondingToast`.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected (al gebeurd, de schijf is weg) → `werd losgekoppeld`** · verleden tijd van het vastgelegde
  `loskoppelen`, zoals `indexing.staleDialog.body` („Terwijl {name} losgekoppeld was”) · `high`. Bewust NIET de lopende
  vorm „wordt losgekoppeld” uit `fileExplorer.navigation.driveIndex.driveLeaving`. „Starts from scratch” → „helemaal
  opnieuw”, in de lijn van `indexing.rescan.incompletePreviousScan` („begint opnieuw met doorzoeken”); `doorzoeken` en
  `mapgroottes` komen uit dezelfde familie.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.destination.copy`)

Vier zinnen over een schijf die er tijdens een kopie of verplaatsing uit werd getrokken. De geruststelling is de kern:
elke waarde moet eindigen op wáár de bestanden staan, niet op wat er misging. `errors.*` is een RAW-familie, dus gewone
apostrofs en geen ICU; geen van de vier waarden bevat er een.

- **was disconnected → `werd losgekoppeld`** · overgenomen van `indexing.needsFreshScan.afterDisconnect` hierboven, de
  al vastgelegde verleden tijd voor een schijf die al weg is · `high`. Bewust NIET de lopende vorm „wordt losgekoppeld”
  van `fileExplorer.navigation.driveIndex.driveLeaving`: die gaat over uitwerpen of ontkoppelen dat nog bezig is,
  terwijl deze vier over een schijf gaan die er zomaar uit is getrokken.
- **„{done} of {total} files” → `{done} van {total} bestanden`, zonder lidwoord** · de telformule van de catalogus
  (`indexing.enrich.progress`, `fileExplorer.imageIndex.folder.someIndexed`, `viewer.pull.progress`) en macOS AppKit
  („Page %ld of %ld” → „Pagina %1$ld van %2$ld”); Thunar `nl` zegt „%s van %s” · `high`. ❌ Geen `van de`: nergens in de
  catalogus staat die vorm.
- **originals → `originelen`; untouched → `onaangeroerd`** · woordelijk uit
  `errors.write.destinationNotFound.message.copy` („The originals are untouched.” → „De originelen zijn onaangeroerd.”)
  en `errors.write.notConnected.message.destination`; `fileOperations.transferProgress.titleRemovingOriginals` gebruikt
  hetzelfde woord voor deze fase van een verplaatsing · `high`.
- **„where they were” → `waar ze stonden`** · `fileOperations.cancelRollback.moveAlreadyLanded` vertaalt precies deze
  Engelse staart („are still where they were” → „staan nog op hun oude plek”) · `high` op de betekenis. Hier
  `waar ze stonden` en niet `op hun oude plek`, want er is geen nieuwe plek: deze bestanden zijn nooit vertrokken.
- **„so nothing is lost” → `dus er is niets verloren gegaan`** · `verloren gaan` is Apples eigen werkwoord hiervoor
  (macOS Finder `FF45`/`FF46` „gaan ze verloren”, AppKit „Je wijzigingen gaan verloren als je ze niet bewaart”) ·
  `high`. De voltooide tijd omdat het hier al gebeurd is, niet dreigt.
- **„The rest are still on the drive” → `De rest staat nog op de schijf.`** ·
  `fileOperations.cancelRollback.stoppedDeleting` („De rest staat er nog.”) en
  `fileOperations.cancelRollback.stoppedMovingBack` („De rest staat nog op de nieuwe plek.”) geven de vorm; `de rest` is
  enkelvoud, dus `staat`. `drive → schijf` is de termregel hierboven · `high`.
- **„before Cmdr could finish the move” → `voordat Cmdr het verplaatsen kon voltooien`** · macOS Finder `NE111` zegt
  precies dit over een overdracht („Je kunt het kopiëren nu voltooien”), met dezelfde naamwoordelijke werkwoordsvorm die
  de catalogus al gebruikt (`fileOperations.transferProgress.scanTitleMove` „Controleren voor het verplaatsen...”) ·
  `high`. `voordat Cmdr klaar was met verplaatsen` uit `errors.listing.connectionDropped.explanation` was het
  alternatief; `voltooien` blijft dichter bij „finish”.
- **Naar de bestemmingsschijf toe: `er … naartoe`, en nooit een lidwoord bij de naam.** `{volumeName}` en
  `{counterpart}` dragen willekeurige schijfnamen, dus geen `de`/`het` ervoor en geen voornaamwoord dat een geslacht
  kiest: `nadat Cmdr er {done} van {total} bestanden naartoe had gekopieerd` · `high`.
- **Woordvolgorde: de geruststelling staat in een hoofdzin.** Het Engels hangt „so nothing is lost” achter een
  `nadat`-bijzin; in het Nederlands duwt die bijzin het werkwoord naar achteren en begraaft ze de geruststelling. Daarom
  twee zinnen, waarvan de tweede met het onderwerp begint en het werkwoord op plaats twee houdt („Je originelen zijn
  onaangeroerd …”, „De rest staat nog op de schijf.”).

## A move that could not be confirmed (`errors.write.moveNotConfirmed.title`)

Geen mislukking: Cmdr kon niet aantonen dat de kopieën waren weggeschreven, en heeft de originelen juist daaróm laten
staan. Nergens mag de tekst zeggen dat het verplaatsen is misgegaan.

- **„Couldn't confirm …” → `kon niet bevestigen`** · het vastgelegde patroon van de catalogus voor precies deze
  onzekerheid (`fileOperations.mkdir.timeoutMessage` „We konden niet bevestigen dat de map is aangemaakt”,
  `fileExplorer.pane.trashUnconfirmedToast`, `fileExplorer.rename.unconfirmed`), en `bevestigen` is Microsoft Tier 2
  (`DUTCH.tbx`) plus macOS AppKit („Confirm” → „Bevestig”) · `high`. Hier `Cmdr kon` in plaats van het `We konden` van
  die drie, want het Engels noemt Cmdr zelf als onderwerp.
- **Kop → `Kon het verplaatsen niet bevestigen`** · de koppenvorm van deze familie laat het onderwerp weg
  (`errors.write.destinationNotFound.title` „Kon de doelmap niet vinden”, `errors.write.sourceNotFound.title` „Kon het
  bestand niet vinden”) · `high`. `het verplaatsen` als naamwoord volgt macOS Finder `NE111` en de catalogus; ❌ niet
  `de verplaatsing`, dat is in de terugdraaironde al afgewezen als UI-vorm.
- **„were saved on {volumeName}” → `op {volumeName} zijn bewaard`** · de termregel `save → bewaren` (macOS gebruikt
  `bewaren`, niet `opslaan`) · `high`. `weggeschreven` uit `errors.write.newDataKeptAt.message` viel af: dat is
  technischer dan het Engelse „saved” en trekt de zin naar de schijfkant in plaats van naar de gebruiker.
- **„it kept your originals where they were” → `dus heeft het je originelen laten staan waar ze stonden`** ·
  `laten staan` is de vastgelegde vorm voor iets wat Cmdr bewust niet verplaatst (termregel
  `left where it is → laten staan`), en `het` als voornaamwoord voor Cmdr staat al in
  `fileOperations.cancelRollback.moveAlreadyLanded` („alles wat het heeft verplaatst”) · `high`. De handelende vorm
  blijft staan: dát Cmdr de originelen bewust heeft laten staan, is de geruststelling zelf. Een lijdende vorm („zijn
  blijven staan”) zou verbergen wie ze beschermde.
- **„Have a look at …” → `Kijk even op …`** · `onboarding.stepBeta.signup.rejected` („Kijk even of er een typefout in
  zit”) en `adb.connect.unauthorized` („Kijk op je telefoon”) · `high`. Het `even` draagt de lichte toon van „have a
  look”; `Controleer` zou hier als een opdracht klinken.
- **„Your originals haven't moved.” → `Je originelen staan nog waar ze stonden.`** · dezelfde staart als de twee
  berichtregels, zodat het venster één belofte doet · `high`. Een letterlijke ontkenning („zijn niet verplaatst”) legt
  de nadruk op de handeling die niet doorging in plaats van op waar de bestanden staan.

## Files a drive brings back from a move that never finished (`fileOperations.leftovers.stagingFolderKept`)

Een informatieve melding als een schijf terugkomt (of Cmdr opstart) en Cmdr de werkmap van een verplaatsing aantreft met
bestanden erin. Geen fout en geen vraag: Cmdr laat alles bewust staan, want het kan het enige exemplaar zijn. De melding
bestaat alleen zodat bestanden die iemand mist een vindbare plek hebben. ❌ Nooit suggereren dat de map weg mag. Let op
het verschil met `fileOperations.cancelRollback.stagedLeftover.named` hierboven: dáár gaat het om Cmdrs eigen restanten
(„Je kunt het gerust verwijderen"), hier om de bestanden van de gebruiker, die Cmdr juist beschermt.

- **„an unfinished move" → `waarvan het verplaatsen niet is voltooid`** · `het verplaatsen` is de vastgelegde
  naamwoordsvorm (macOS Finder: „zoals het verplaatsen of kopiëren van een onderdeel"), en `niet voltooid` is wat de
  catalogus over een gestrande bewerking zegt (`queue.failureToast.title`: „Verplaatsen niet voltooid") · `high`. ❌
  Geen `een verplaatsing die niet is voltooid`: `de verplaatsing` is al afgewezen als UI-vorm, en het pile bevestigt dat
  (geen enkele treffer in `nl/macOS/`, en de bestandsbeheerders kennen het woord alleen in de betekenis „beweging"). ❌
  Ook niet `bestanden die niet helemaal zijn verplaatst`: dat leest als half geschreven bestanden en botst met het
  `onvolledig exemplaar` van de `stagedLeftover`-broers, terwijl deze bestanden juist gaaf zijn.
- **`voltooid`, niet `afgerond`** · beide staan in de catalogus, maar `afgerond` hoort bij het doorzoeken en bij een
  antwoord (`indexing.rescan.incompletePreviousScan`, `askCmdr.error.unfinishedReply`) en `voltooid` bij de
  bestandsbewerkingen zelf (de hele `queue.failureToast.title`-selectie) · `high`.
- **„left them in place" → `heeft ze allemaal laten staan`** · termregel `left where it is → laten staan`, de vorm voor
  iets wat Cmdr bewust niet aanraakt (`errors.write.moveNotConfirmed.message.named` gebruikt haar al) · `high`. Bewust
  géén `waar ze stonden` zoals bij `moveNotConfirmed`: die originelen zijn nooit vertrokken, deze bestanden staan juist
  op hun nieuwe plek. `allemaal` vervangt het Engelse „in place", dat naast de maplocatie dubbelop zou zijn, en draagt
  de belofte van de `@key` („every one of them") die anders in het Nederlands verdampt.
- **„a hidden folder named {folderName}" → `een verborgen map met de naam {folderName}`** · `met de naam` is de vorm van
  macOS Finder („Er bestaat al een map met de naam '^0' op deze locatie") en staat al in de catalogus
  (`errors.mount.shareNotFound`, `errors.write.duplicateSourceNames.message`) · `high`. `genaamd` komt in het hele pile
  niet voor.
- **`hidden` → `verborgen`** · macOS („wordt het bestand een verborgen bestand"), Nautilus („verborgen mappen"), en de
  catalogus zegt het al zo (`menu.view.showHiddenFiles`, `fileExplorer.rename.hiddenAfterRename`,
  `settings.listing.showHiddenFiles.description`) · `high`. Geen verbuigingsvraag bij `map`: een bijvoeglijk gebruikt
  voltooid deelwoord op `-en` krijgt nooit een extra `-e`.
- **De plaatshouder-ontwijking: `op {volumeName}` en `met de naam {folderName}`, allebei zonder lidwoord.** Beide dragen
  willekeurige tekst, dus geen `de`/`het` en geen verwijzend voornaamwoord dat een geslacht moet kiezen — dezelfde regel
  als bij `errors.write.deviceDisconnected.sided.source.move` · `high`.

## Het favorietenmenu (`commands.favoritesOpen.label`/`.description`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`/`.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

⌃D opent de favorieten als een menu over het actieve paneel: de eerste negen rijen dragen de cijfers 1–9, de laatste rij
het cijfer `0` en voegt de huidige map toe. De volumekiezer heeft geen favorietensectie meer, alleen nog één bovenste
rij die naar dit menu springt. De term `favorieten` lag al vast in de catalogus
(`fileExplorer.navigation.groupFavorites`, `fileExplorer.navigation.favoritesEmpty`, `commands.favoritesAdd.label`,
`menu.go.addToFavorites`); deze pass munt er geen tweede woord voor.

- **„favorites" → `favorieten`** · macOS Tier 1 door de hele bundel heen (Finder `Favorieten`, `Favoriete servers:`, „De
  server '^0' kan niet aan je favorieten worden toegevoegd.", AppKit `Zet in favorieten` / `Verwijder uit Favorieten`,
  Systeeminstellingen `Favorieten`) en al de catalogusterm · `high`. Kleine letter in een werkwoordszin
  (`Toon favorieten`), hoofdletter alleen waar het een kopje is (`Favorieten` als groepsnaam).
- **„favorites menu" → `favorietenmenu`, aaneen** · Nederlandse samenstellingen plakken (stijlgids § Notes and
  decisions), macOS doet het net zo met `locatiemenu` en `venstermenu`, en de zusterkopjes in dezelfde lijst zijn
  eveneens samenstellingen (`shortcuts.scope.volumeChooser` → `Volumekiezer`, `shortcuts.scope.fileList` →
  `Bestandenlijst`, `shortcuts.scope.commandPalette` → `Opdrachtenpalet`) · `high`. Het streepje uit `het Help-menu`
  hoort bij de Engelse eigennaam ervoor en geldt hier dus niet.
- **„Show favorites" → `Toon favorieten`**, in zowel het native Ga-menu als het opdrachtenpalet · macOS rendert `Show X`
  consequent als `Toon X` (AppKit `Show Details` → `Toon details`, `Show Fonts` → `Toon lettertypen`, `Show Sidebar` →
  `Toon navigatiekolom`), en de catalogus doet dat al bij `menu.go.showServers` (`Toon servers`) · `high`. De twee
  sleutels zijn bewust byte-identiek: `menu.go.showFavorites` is de native tweeling van `commands.favoritesOpen.label`.
  `menu.go.showFavorites` is een RAW-sleutel, maar de waarde draagt geen apostrof, dus er valt niets te verdubbelen.
- **„See {count} favorites" → `Bekijk {count} favorieten`, met `Bekijk` en niet `Toon`** · het Engels zet hier bewust
  „See" tegenover het „Show" van de opdracht, en de catalogus kent die scheiding al: `commands.logOperationLog.label`
  houdt `Bekijk` voor alleen-kijken, terwijl `Toon` bij het tevoorschijn halen van een oppervlak hoort (stijlgids §
  Notes and decisions, de `beoordelen`/`bekijken`-regel) · `high`. Stam-imperatief, want de rij is een echte knop.
- **Meervoudsvorm van `fileExplorer.navigation.seeFavorites`: `=0` + `one` + `other`** · dat zijn de echte
  CLDR-categorieën van het Nederlands (`new Intl.PluralRules('nl')`, ook vastgelegd in de stijlgids § Plurals), en ze
  vallen hier toevallig samen met de drie armen van het Engels. `=0` houdt de „je hebt er nog geen"-tekst zonder cijfer
  (`Bekijk favorieten`), `one` en `other` dragen allebei `{count}`. Geen werkwoordsval zoals bij `is`/`zijn`: een
  imperatief heeft geen onderwerp dat meetelt · `high`.
- **„current folder" → `huidige map`, zonder lidwoord in de menurij en mét lidwoord in een hele zin** · de catalogus
  laat het lidwoord al weg in korte labels (`commands.fileCopyCurrentDirectoryPath.label` →
  `Kopieer pad van huidige map`, `queryUi.scope.useCurrentFolder` → `Gebruik huidige map`) en macOS doet hetzelfde („Ga
  naar map in huidige locatie", „Dupliceert onderdelen op huidige locatie") · `high`. In een hele zin blijft het
  lidwoord staan, zoals `commands.favoritesAdd.description` al doet.
- **De `0`-rij heeft nog maar ÉÉN sleutel: `fileExplorer.navigation.favoritesAddCurrent`**
  (`Voeg huidige map aan favorieten toe`) · `high`. De sneltoetsenlijst CITEERT die rij om de `0`-toets uit te leggen en
  leest dus dezelfde waarde. ❌ Geen tweede, lidwoord-dragende formulering meer verzinnen voor die lijst; het verschil
  dat wél telt, is dat met het echte commando `commands.favoritesAdd.label` (`Voeg aan favorieten toe`).
- **Het scheidbare partikel gaat naar het eind, ook met een lijdend voorwerp ervoor**:
  `Voeg huidige map aan favorieten toe` · de knopregel van de stijlgids § Formality mechanics, en macOS AppKit zegt het
  net zo („Voeg het lettertype aan de stijl toe") · `high`. Zo blijft de rij in dezelfde familie als
  `commands.favoritesAdd.label` (`Voeg aan favorieten toe`) en `menu.go.addToFavorites`.
- **„the favorite with that number" → `de favoriet met dat nummer`, en de toets zelf is een `cijfer`** ·
  `commands.favoritesOpenByNumber.label` gaat over het nummer dat naast de rij staat, terwijl
  `commands.favoritesOpen.description` over de toetsaanslag gaat, en het Nederlands scheidt die twee: een `nummer`
  identificeert, een `cijfer` is het teken op de toets · `high`. De catalogus zegt `druk op` voor een toetsaanslag
  (`downloads.toast.inAppHint`, `goToPath.toast.pressToGoBack`, `fileExplorer.edit.notOnThisMac`), dus
  `druk op een cijfer om naar die favoriet te springen`; `springen` is de vastgelegde vorm voor navigeren met een paneel
  (`commands.navGoToPath.description`).
- **„This folder is already a favorite" → `Deze map staat al in je favorieten`** · een herstructurering: het Engels zegt
  wát de map is, het Nederlands zegt wáár ze staat, wat vlotter leest en het al vastgelegde bezittelijke kader
  hergebruikt (`fileExplorer.navigation.favoritesEmpty` → „(Je favorieten verschijnen hier)", Finder: „kan niet aan je
  favorieten worden toegevoegd") · `high`. Rustige mededeling, geen fout: geen `lukt niet`, geen uitroepteken.
- **„a mounted share" → `een gekoppelde netwerkshare`** · de catalogus staat al helemaal op `koppelen` voor _mount_
  („Dit volume is alleen-lezen gekoppeld", „nadat … de share opnieuw is gekoppeld", „de momenteel gekoppelde volumes",
  `errors.listing.readOnlyVolumeErrno.explanation`, `errors.listing.staleConnection.suggestion`) en op `netwerkshare`
  voor _share_ (`errors.eject.notAnSmbVolume`: „Dit is geen netwerkshare") · `high`. macOS zelf zegt `activeren`
  („Activeer het volume", „kon niet worden geactiveerd", AppKit `Document.json`), maar dat woord staat nergens in deze
  catalogus en zou naast al het bestaande `losgekoppeld`/`aangekoppeld` een tweede term voor één handeling zijn. Het
  bijvoeglijke `gekoppelde` is dragend: het onderscheidt een share die de Mac zelf aankoppelt van een server waarmee
  Cmdr zelf verbindt. ❌ Geen protocolnamen (`SMB`, `MTP`, `ADB`) erbij halen; het Engels vermijdt ze bewust.
- **`favoritesCantAddHere` gaat over DEZE map, de reden komt na de dubbele punt** ·
  `Deze map kan geen favoriet worden: favorieten werken alleen op schijven en gekoppelde netwerkshares` · `high`. Zelfde
  onderwerp als de zusterrij `fileExplorer.navigation.favoritesAlreadyAdded` („Deze map staat al in je favorieten"), dus
  de twee grijze regels lezen als een paar. ❌ Niet meer `wijzen naar`: dat dwong elke verbuigende taal een naamval voor
  het doel te kiezen en maakte de regel fors langer dan het Engels; `werken op` is een vlakke plaatsbepaling. Geen punt
  aan het eind (tooltip), geen `fout` of `mislukt`.
- **`commands.favoritesAdd.description` noemt niet meer de favorietensectie van de wisselaar** (die M3 heeft
  weggehaald), maar waar de map echt heen gaat:
  `Voeg de huidige map van het actieve paneel toe aan je favorieten, zodat je er via het favorietenmenu weer terugkomt.`
  · `high`.
- **`commands.favoritesOpen.description` blijft ongewijzigd**: het Engels zei eerst alleen „press a number to go", en
  deze vertaling vulde de bestemming al aan („om naar die favoriet te springen"). Het Engels noemt die bestemming nu
  zelf, dus de Nederlandse zin klopte al en alleen de `sourceHash` liep achter · `high`.

## Wie de schijf vasthoudt: de zes geweigerde-uitwerpzinnen (`errors.eject.unmountRefusedBy*`, `.otherApps`)

Zes zinnen die de generieke `errors.eject.unmountRefused` opvolgen zodra Cmdr wél weet wie de schijf vasthoudt: één app,
meerdere apps, een schijfkopie, macOS zelf, of Cmdr zelf. Ze vallen achter dezelfde dubbele punt als de negen zinnen
hierboven (`{volumeName} uitwerpen lukte niet: …`), dus dezelfde regels gelden: geen herhaling van het wikkelwerkwoord,
en `errors.*` is RAW, dus enkele apostrofs. Geen van de zes waarden draagt er een.

- **De hele familie deelt één werkwoord: `gebruikt … nog`.** De generieke broer zegt `Deze schijf is nog in gebruik`;
  zodra er een naam bekend is, wordt dat een actief `{app} gebruikt deze schijf nog`, met `{apps} gebruiken` in het
  meervoud. Apple gebruikt beide vormen (`… is in gebruik`, en `in gebruik door een andere app` in het
  verplaats-waarschuwingsvenster van de Finder), dus de keuze valt op wat de naam vooraan zet · `high`.
- **`{app}` staat vooraan zonder lidwoord.** Een procesnaam is een eigennaam, dus Nederlands zet er net zomin een
  lidwoord voor als Engels: `Voorvertoning gebruikt deze schijf nog`, `mds_stores gebruikt deze schijf nog`. Vooraan
  zetten is bewust: in een korte melding is de naam het enige wat de lezer nodig heeft om iets te kunnen doen · `high`.
  Het alternatief `Deze schijf is nog in gebruik door {app}` is even goed Nederlands maar begraaft de naam achteraan.
- ❌ **Geen voornaamwoord dat naar `{app}` terugwijst.** Het Engels zegt „anything **it** has open there"; een
  procesnaam kan in het Nederlands een de- of een het-woord zijn, dus elk voornaamwoord is de helft van de tijd fout (de
  regel uit `style.md` § Notes and decisions). Daarom `Sluit alles wat daar openstaat`: het plaatsbijwoord `daar` wijst
  naar de schijf, die wél een vast geslacht heeft. Bonus: enkelvoud en meervoud kunnen zo dezelfde tweede zin delen ·
  `high`.
- **`other apps` → `andere apps`, niet `andere programma's`** · de map `Applications` heet in het Nederlandse macOS
  `Apps` (zie § Het Dock-aanbod en `style.md`), Finder schrijft `Stop geopende apps` en
  `in gebruik door een andere app`, en de catalogus zegt al overal `apps` · `high`. `Programma's` leeft in het
  Nederlandse macOS alleen nog in `Hulpprogramma's`. Meegenomen voordeel: `andere apps` draagt geen apostrof, dus de
  RAW-familie kan er niets aan verkeerd doen.
  - De lijstvoegwoorden komen van `Intl.ListFormat('nl')`, niet uit de string:
    `Voorvertoning, Warp, Foto's en andere apps` (nagemeten met Node, 2026-09-16). `andere apps` heeft op die laatste
    plek geen naamvals- of lidwoordaanpassing nodig, dus de waarde blijft de kale zelfstandignaamwoordgroep.
- **disk image → `schijfkopie`** · macOS Finder `InfoWindowGeneralView` (`tvy-hx-Gou.title` = `Schijfkopie:`) · `high`.
  Een de-woord. De zin herhaalt het naamwoord (`Werp eerst die schijfkopie uit en daarna deze schijf`) in plaats van een
  `die` te gebruiken, want `schijf` is óók een de-woord en zou de verwijzing dubbelzinnig maken; dezelfde regel als bij
  `Open de server opnieuw` in `style.md`.
- **„is still open" → `staat nog open`** · gemunt, want het pile kent `openstaan` niet voor bestanden. Het Engels kiest
  bewust het alledaagse „open" boven het technische „mounted", en `staat nog open` draagt precies diezelfde lichtheid;
  het technisch preciezere `is nog gekoppeld` zou het register optillen · `tentative`. In de bijzin wordt het één woord
  (`wat daar openstaat`).
- **macOS blijft `macOS`, ook aan het begin van de zin**, met de kleine letter die de merknaam draagt, net als in het
  Engels. Geen verbuiging nodig: `macOS is nog met deze schijf bezig`, met `bezig` achteraan zoals Nederlands dat wil
  (Apple schrijft `iCloud is bezig met synchronisatie`, dezelfde constructie met een langere bepaling) · `high`.
- **„Wait a minute" en „Wait a moment" blijven twee verschillende wachttijden** · `Wacht een minuutje` voor macOS (dat
  echt even bezig kan zijn met indexeren) en `Wacht even` voor Cmdr zelf. Het Engels maakt datzelfde onderscheid, en
  `minuutje` past bij de informele toon · `high`.
- **„send a report" → `verstuur een rapport`** · de vastgelegde `versturen` plus `rapport` (`crashReporter.dialog.send`
  = `Verstuur rapport`) · `high`. „if it keeps happening" → `als het blijft gebeuren`, letterlijk de vorm van
  `errors.serverRequest.unexpected` · `high`.
- **`Cmdr zelf gebruikt deze schijf nog`**: `zelf` staat direct achter het onderwerp, waar het Engelse „itself" ook
  staat, zodat de zin de schuld meteen bij Cmdr legt in plaats van halverwege · `high`.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `Soort`** · macOS Finder `nl`
  `ArrangeByMenu` `119.title`/`338.title`, the Kind sort criterion · `high`.
- **"Select all with extension `*.{extension}`" → `Selecteer alles met extensie *.{extension}`** · Double Commander
  (`tfrmmain.actmarkcurrentextension.caption`, "Select All with the Same Extension" →
  `Kies alles met hetzelfde achtervoegsel`) and Total Commander (`WCMD.INC` `527` →
  `Alle bestanden met dezelfde extensie markeren`) name this exact command, and the mask replaces their "same extension"
  because Cmdr shows the concrete one · `high`. Het masker staat na `met extensie`, dus er hoeft niets met `{extension}`
  te congrueren. `extensie` (macOS) wint van DC’s `achtervoegsel`, zoals de extension-regel hierboven al vastlegde.
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `Selectie`** · the catalog's settled noun for
  the SET of selected files, from `commands.selectionSelectFiles.description` („aan de selectie toe te voegen”) ·
  `high`. Its siblings in that menu are verbs; this one names what the submenu holds. ❌ Not the verb `Selecteer`, which
  is `menu.bar.select`.
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## The title-bar full-disk-access badge

De waarschuwingspil in de titelbalk plus de tooltip, zichtbaar zolang Cmdr geen volledige schijftoegang heeft; een klik
heropent de onboarding op stap 1.

- **`onboarding.fdaBadge.label` → `Geen volledige schijftoegang`** · Apple's own Systeeminstellingen row, now confirmed
  against the live macOS bundle
  (`/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable`, key
  `ALL_FILES`, macOS 27.0 build 26A428, verified 2026-09-16) · high. The existing term already had it right.
- **`onboarding.stepAi.bannerTitle.denied` already carried this exact string**, and `i18n-terms` holds the two identical
  because they share one English source. ❌ Reword neither alone.
- **`onboarding.fdaBadge.ariaLabel` opens with the label verbatim**
  (`Geen volledige schijftoegang. Open de stap over volledige schijftoegang in de onboarding.`), which is what satisfies
  `i18n-aria` (WCAG 2.5.3). ❌ Re-wording the label alone breaks it.
- **Tooltip terms**: drive → `schijf` (settled), scan/search a drive → `doorzoeken` (settled), cloud folders →
  `cloudmappen` (the compound, like the settled `cloudprovider`), "files macOS keeps to itself" →
  `bestanden die macOS voor zichzelf houdt` (plain, ❌ never a macOS feature name), "Click to …" → the bare-stem
  imperative `Klik om …` (as in `fileExplorer.breadcrumb.navigateTooltip`), onboarding → `onboarding` (settled loan).

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

macOS turned down a move to the trash, and Cmdr now words the refusal three ways (permission-shaped, no trash at that
location, unclassified) instead of one sentence. RAW family, so single apostrophes and `{count}` is a literal
replacement target. Four rules bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical. ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. Dutch solves it
  with `{count} van de onderdelen die je koos`, which takes any numeral without agreement.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails`
  (`Technische details`), because it points at that very control.

- **locked → `beveiligd`** · Apple's own Dutch word for the state and the checkbox (macOS Finder `AXNODE1` `Beveiligd`),
  and the settled `locked` term · high. ⚠️ The older siblings in this same dialog (`errors.write.fileLocked.title`,
  `errors.write.permissionDenied.suggestion.deleteMac`) still say `vergrendeld`; that sweep is still open, and this key
  is written on the recommended side of it.
- **"delete them permanently" → `definitief verwijderen`** · matches `commands.fileDeletePermanently.label`
  (`Verwijder definitief`), so the suggestion names the command the user will run · high.
- **badge (the title-bar pill) → `de markering`** · reuses the settled `badge / status badge → markering` · high. title
  bar → `titelbalk` · high.
- The quoted badge text is `onboarding.fdaBadge.label` verbatim, in the catalog's `‘…’` quotes.
- "somewhere macOS keeps to itself" → `op een plek die macOS voor zichzelf houdt`, reusing the wording settled for
  `onboarding.fdaBadge.tooltip`. Plain, ❌ never a macOS feature name.

## De waarschuwing voor alleen-online inhoud (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Is een geselecteerd item in een cloudmap alleen online beschikbaar, dan zou de prullenmand het eerst downloaden. Daarom
opent Cmdr het venster voor definitief verwijderen en legt dat uit in de banner. Twee bannervarianten: één voor een
gemengde selectie, één voor een selectie die volledig alleen online is. Ze verschillen alleen in de eerste zin en in de
uitwegen die ze kunnen noemen. De derde sleutel is de regel die verschijnt als Cmdr een druk teruggeeft.

- **`.cloudOnlineOnlyMixedWarning`** · `alleen online beschikbaar` (de term `online-only`), `prullenmand` en
  `cloudservice` · medium. **De download als gevolg, niet als handeling van het verplaatsen**:
  `Als je die onderdelen naar de prullenmand verplaatst, worden ze <strong>eerst gedownload</strong>.` ❌ Niet het
  calque `Verplaatsen naar de prullenmand zou ze eerst downloaden`, waarin het verplaatsen zelf downloadt. Het Engelse
  "In this case" wordt `In dit geval`, geen causaal `Daarom`. De eigen bewaartermijn van de service heet
  `bewaren verwijderde bestanden`, geen `prullenmand`.
- **`.cloudOnlineOnlyAllWarning`** · dezelfde tekst, met «Alles wat je hebt geselecteerd» in plaats van «Een deel van je
  selectie», en zonder de uitweg deselecteren: als alles alleen online is, blijft er niets geselecteerd · medium.
- **`.cloudOnlineOnlyHandedBack`** · de regel boven de knop na een druk die Cmdr bewust niet heeft uitgevoerd. Zakelijk,
  zonder excuses · medium. `druk er daarna nog een keer op` (het werkwoord `drukken op` houdt zijn `er … op`), en
  `bleek` (verleden tijd, zonder "toch").
- **Alle vier de feiten blijven staan**: (1) de prullenmand zou de bestanden downloaden, (2) daarom biedt Cmdr alleen de
  HELE selectie verwijderen aan, (3) daarna blijft er GEEN kopie in de prullenmand, de service houdt wel een eigen kopie
  (❌ niet afzwakken), (4) de uitwegen die de banner noemt.
- **De twee `<strong>`-stukken blijven**, op «eerst downloaden» en op het werkwoord «verwijderen». En ‘Verwijder’ tussen
  aanhalingstekens is het label van de knop: altijd hetzelfde als `fileOperations.delete.confirmDelete`.
- Bekijken bij de overflow-controle: de banner is lang en staat in een smalle strook boven de bestandenlijst.
- ⚠️ Concept, nog niet door een mens nagelezen.

## Als de server zegt dat die gedeelde map er niet is (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Het enige geval in deze familie waarin opnieuw proberen niets oplost: de server antwoordt duidelijk dat hij geen
gedeelde map met die naam heeft. Daarom staat er geen knop bij deze melding, en mag de toon niets tijdelijks suggereren
(geen `nu`, geen `probeer het opnieuw`), anders dan bij de zusjes.

- **"the server says it has no share by that name" →
  `want de server zegt dat hij geen gedeelde map met die naam heeft`** · de catalogus (`errors.mount.shareNotFound`) en
  NetAuthAgent `EINFO_NO_SHARE` (LIVE-bundel, macOS 26.6.2, 25G83, 2026-09-17) · high. ⚠️ Apple zegt daar
  `De share '%@' bestaat niet op de server`; de catalogus houdt `gedeelde map` aan, zoals de sectie over
  `errors.mount.*` al vastlegde.
- **"This one won't sort itself out" → `Dit lost zichzelf niet op`** · geen bron in de stapel; het is de gewone
  Nederlandse wending en draagt precies wat deze melding van de rest onderscheidt: wachten helpt niet · high.
- **"may have been renamed or removed" → `is misschien van naam veranderd of verwijderd`** · letterlijk uit
  `errors.write.destinationNotFound.suggestion` · high.
- **"so it's worth checking there" → `dus daar kun je het beste even kijken`** · `je`-register van `style.md`; `daar`
  verwijst naar de server, zodat het woord niet nog eens hoeft · high.
- **"a lot slower" → `een stuk langzamer`** · het Engels geeft hier geen factor, anders dan het zusje met `4x` · high.
- **In de korte melding verwijst `die` naar `deze gedeelde map`**, niet naar `{server}` · `hij` zou tussen twee
  de-woorden blijven zweven · high. Het slot `blijft op de systeemverbinding` is dat van de drie zusjes
  (`fileExplorer.pane.directConnectionUnreachableToast`…).

## Namen die er hetzelfde uitzien (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.title`, `errors.volume.ambiguousName`)

Een server kan twee namen bewaren die op het scherm identiek zijn (een `é` als één teken of als `e` plus accent, of
alleen een verschil in hoofdletters). De eerdere Nederlandse versie van de `ambiguousName`-sleutels kwam van een
programmeeragent die deze gids niet had gelezen; deze pas trekt ze recht.

- **item → `onderdeel`, ook hier** · de gewone term (macOS Finder `items` → `onderdelen`) · high. Geldt voor de hele
  catalogus, ook voor een onderdeel in Sleutelhangertoegang (`ai.secretError.keychainBody`): Keychain Access `nl` zegt
  `Nieuw wachtwoordonderdeel…`, `Alle onderdelen`, `Dit onderdeel is onbeperkt toegankelijk.` (`MainMenu.loctable` /
  `Localizable.loctable`, macOS 27, 2026-09-23) · high. `item` blijft alleen staan waar het geen bestand of map is:
  `menu-item`, `stash-item` (een git-stash, `fileExplorer.git.size.stashEntries`), en de geschiedenisregels in
  `settings.*.recent*.maxCount.description` ("entries" van een lijst zoekopdrachten of selecties).
- **"look the same" → `zien er hetzelfde uit`** · gewone Nederlandse wending, geen bron nodig · high.
- **"the server spells them differently" / "stores them spelled differently" → `de server slaat ze anders op`** · de
  `@key`-beschrijving vraagt om gewone woorden voor "andere tekenreeks"; `spellen` voor een server leest vreemd en
  `verschillend gespeld opslaan` is een Engelse constructie · high. Binnen de uitleg tussen haakjes wordt het
  `kan op twee manieren worden geschreven`, zodat `opslaan` niet twee keer in één zin staat.
- **"matches" (een pad dat bij meer onderdelen hoort) → `past bij`** · kort en ongeacht of `{path}` een bestand of map
  is · tentative (geen bron voor deze zin in de stapel; `komt overeen met` is het formelere alternatief).
- **upper and lower case → `hoofdletters en kleine letters`** · macOS (`Negeer hoofdletters`, `Hoofdletters`) · high.
- **De knopnaam in een zin: `Met ‘Overschrijf’ vervang je …`** · de knop heet `Overschrijf`
  (`fileOperations.transferProgress.conflictOverwrite`); een stam-imperatief als onderwerp leest vreemd, dus de knopnaam
  staat tussen enkele krulaanhalingstekens achter `Met` en het werkwoord wordt `vervangen` (Finder `Replace` →
  `Vervang`) · high.
- **"the one that's there" → `het onderdeel dat er al staat`** · `onderdeel` is een het-woord en dekt bestand én map ·
  high.
- **"Choose it from its folder instead" → `Open de map waarin het staat en kies het daar`** · `in plaats daarvan` was
  letterlijk en stroef; `het` verwijst naar `onderdeel` · high. `{path}` staat, zoals in alle `errors.volume.*` -zusjes,
  tussen `‘…’`, niet tussen rechte `"…"`.
- **"To avoid this next time, rename one of them" → `Voorkom dit de volgende keer: wijzig de naam van een van de twee`**
  · rename in lopende tekst is `de naam wijzigen` (Finder), en de lijst opent elders ook met een imperatief · high.

## De schakelaar ‘Cloud-AI toestaan’ en de toestanden waarin cloud-AI uit staat (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

Een privacyschakelaar: zolang hij uit staat, stuurt Cmdr niets naar een cloud-AI-service. De toon is rustig en belooft
nooit meer dan Cmdr doet. De referentiestapel stond niet op de vertaalmachine; elke keuze leunt op termen die al in
`terms.json` staan en op de catalogus.

- **Allow cloud AI (naam van de schakelaar) → `Cloud-AI toestaan`** · `Cloud-AI` is de optie
  `settings.ai.provider.opt.cloud`, `toestaan` komt uit MS en AOSP (allow → `toestaan`, hierboven vastgelegd), en de
  vorm `<object> toestaan` volgt `settings.fileOperations.allowFileExtensionChanges.label` · `high`. Geciteerd met `‘…’`
  (`settings.ai.cloudConsent.lockedHint`). Waar het Engels ‘Allow cloud AI’ als werkwoord gebruikt
  (`askCmdr.gate.cloudOff.body`, `settings.askCmdr.cloudOffHint`), staat `Sta … cloud-AI toe`: dezelfde woorden,
  vervoegd.
- **cloud AI (in een zin) → `cloud-AI`**, een de-woord: verwijs terug met `die` of herhaal het woord
  (`Sta cloud-AI toe bij Instellingen > AI`), niet met `het` · `high`.
- **‘X is off’ → `X staat uit`** · zoals `servers.hub.discoveryOff` · `high`.
- **Turn on Ask Cmdr (knop) → `Zet Ask Cmdr aan`** · de gebiedende knopvorm, zoals
  `fileExplorer.navigation.driveIndex.menuEnable` · `high`.
- **Open AI settings → `Open AI-instellingen`** · zoals `commands.appSettings.label` (‘Open instellingen’) · `high`.
- **side panel → `zijpaneel`** · `tentative` (geen precedent in de catalogus).
- **custom endpoints → `aangepaste eindpunten`** · `Aangepast` voor custom (macOS), `eindpunt` uit
  `onboarding.cloudSetup.hint.azureEndpoint` · `high`.
- **Ask Cmdr chats → `Ask Cmdr-chats`** · zoals `Ask Cmdr-instellingen` in `ai.cloudConsent.askCmdr.memory` · `high`.
- `settings.askCmdr.enabled.label` is nu alleen ‘Ask Cmdr’ (de productnaam op de schakelaar) en heeft een
  `sameAsSourceJustification`.

## Escape en de schermvullende weergave (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

Een eenmalige informatiemelding nadat Escape het hoofdvenster uit macOS' schermvullende weergave haalde, plus de
schakelaar bij Instellingen > Geavanceerd. Gemijnd in `_ignored/i18n/nl/macOS/`, 2026-09-23.

- **full screen → `schermvullende weergave`** (zelfstandig naamwoord), bijvoeglijk `schermvullend` · macOS AppKit
  (`Enter Full Screen`→`Schakel schermvullende weergave in`, `Exit Full Screen`→`Schakel schermvullende weergave uit`,
  `Full Screen`→`Schermvullend`, `Make Window Full Screen`→`Maak venster schermvullend`) · `high`.
- **Exit full screen on Escape (schakelaar) → `Schermvullende weergave uitschakelen met Escape`** · Apples werkwoord
  `uitschakelen` voor exit, in de infinitiefvorm die de andere Geavanceerd-schakelaars dragen
  (`Tijdelijke bestanden van andere apps tonen`). De melding hergebruikt het label byte-identiek · `high`.
- **Escape (de toets) → `Escape`** · macOS (`Escape`→`Escape`, en in een zin `de Escape-toets`); de korte vorm volgt het
  Engels, dat de toets kaal noemt · `high`.
- **took Cmdr out of full screen → `heeft Cmdr uit de schermvullende weergave gehaald`**; in de hulptekst
  `haalt Escape het daaruit` · gemunt, geen precedent in de stapel · `tentative`.
- **You'll only see this once → `Je ziet dit maar één keer.`** · `tentative`.

## Wachtregels in "Open met" en "Deel" (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- **Finding apps… → `Apps zoeken…`** · infinitief als voortgangsregel, zoals macOS ("Searching…"→"Zoeken…") en
  `settings.behavior.textEditorApp.checking` (`Je apps controleren…`) · `high`.
- **share options → `deelopties`** · samenstelling met de stam van het submenu `Deel`; geen precedent in de stapel ·
  `tentative`.
- **No share options → `Geen deelopties`** · macOS-patroon voor een leeg menu ("No Services Apply"→"Geen voorzieningen
  van toepassing") · `high`.
