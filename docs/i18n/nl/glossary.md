# nl glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/nl/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

From the `fileExplorer.json` pass (mined `_ignored/i18n/nl/`, 2026-06-21):

- connect → Verbind (button) / verbinden, verbinding maken · macOS Finder ("Connect to server"→"Verbind met server",
  "Connect As…"→"Verbind als…") · high
- connecting → Verbinden… · macOS AppKit ("Connecting…"→"Verbinden…") · high
- connected → Verbonden · macOS AppKit ("Connected"→"Verbonden") · high
- connection failed → Verbinding mislukt · macOS AppKit ("Connection Failed"→"Verbinding mislukt"); but Cmdr error rule
  avoids bare "mislukt", so phrase as "Verbinding maken lukte niet" / "Verbinding kon niet tot stand komen" in error
  copy · high
- disconnect → Verbreek (button) / verbinding verbreken · macOS ("Disconnect"→"Verbreek") · high
- server → server · macOS Finder ("Connect to server"→"Verbind met server") · high
- share (network share, noun) → gedeelde map · macOS Finder ("Shared folder"→"Gedeelde map") · high
- host → host · no native macOS term for SMB host; kept as the technical term (matches "hostnaam") · tentative
- mount (verb) → aankoppelen · Double Commander ("unmounted"→"niet-aangekoppelde"); macOS uses "koppel" for disks · high
- volume → volume · macOS Finder ("Volume"→"Volume") · high
- eject → Werp uit (button "Werp {name} uit") / uitwerpen · Nautilus, KDE Dolphin, and Microsoft all use "Uitwerpen";
  macOS Finder's own eject is "Verwijder"/"verwijder media", which collides with delete, so the corroborated "uitwerpen"
  is clearer for Cmdr · high
- favorites → favorieten · macOS Finder ("favorites"→"favorieten") · high
- password → wachtwoord · macOS ("Password"→"Wachtwoord") · high
- username → gebruikersnaam · macOS/MS standard · high
- sign in / log in → Log in (button) / inloggen · macOS Finder ("Log In…"→"Log in…") · high
- cancel → Annuleer (button) · macOS ("Cancel"→"Annuleer") · high
- back → Terug · standard NL UI · high
- retry / try again → Probeer opnieuw · macOS Finder ("Probeer het opnieuw") · high
- refresh → Ververs / Verversen · Double Commander ("Refresh"→"Verversen") · high
- search → Zoek (button) / zoeken · macOS Finder ("Search"→"Zoek") · high
- search results → zoekresultaten · macOS Finder ("matches"→"zoekresultaten") · high
- name → Naam · macOS Finder ("Name"→"Naam") · high
- size → Grootte · macOS Finder ("Size"→"Grootte") · high
- modified → Bewerkingsdatum · macOS Finder ("Date Modified"→"Bewerkingsdatum") · high
- created → Aanmaakdatum · macOS Finder ("Date Created"→"Aanmaakdatum") · high
- rename → Wijzig naam (action) / naam wijzigen · macOS Finder ("Rename"→"Wijzig naam") · high
- new folder → Nieuwe map · macOS Finder ("New Folder"→"Nieuwe map") · high
- new file → Nieuw bestand · compound of "nieuw" + macOS "bestand" · high
- tab → tabblad (plural tabbladen) · macOS Finder ("New Tab"→"Nieuw tabblad") · high
- read-only → alleen-lezen · macOS Finder ("Read Only"→"Alleen lezen"; compound hyphenated as "alleen-lezen") · high
- pane → paneel (left/right → linkerpaneel/rechterpaneel) · Double Commander ("left panel"→"linkerpaneel") · high
- file list → bestandenlijst · Double Commander ("file list"→"bestandenlijst") · high
- item(s) → onderdeel/onderdelen · macOS Finder ("items"→"onderdelen") · high
- delete → Verwijder / verwijderen · macOS Finder ("Delete"→"Verwijder") · high
- move → Verplaats / verplaatsen · macOS Finder ("move"→"verplaatsen") · high
- folder → map (plural mappen) · macOS Finder ("folder"→"map") · high
- index / indexing → index / indexeren · standard NL technical term · tentative
- credentials → inloggegevens · standard NL (macOS uses "naam en wachtwoord" descriptively) · tentative
- guest → gast · standard NL · high
- hostname → hostnaam · NL compound of host + naam · tentative
- timeout → time-out · standard NL spelling · tentative
- unreachable → onbereikbaar · standard NL · high

From the `settings.json` pass (mined `_ignored/i18n/nl/`, 2026-06-21):

- default → Standaard · macOS ("Default"→"Standaard") · high
- custom → Aangepast · macOS ("Custom"→"Aangepast") · high
- system default → Systeemstandaard · compound of macOS "Standaard" + "systeem" · high
- sort by → Sorteer op · macOS Finder ("Sort By"→"Sorteer op") · high
- view (mode/menu) → Weergave · macOS ("View"→"Weergave") · high
- color → kleur · macOS ("Color"→"kleur") · high
- none → Geen · macOS ("None"→"Geen") · high
- normal → Normaal · macOS ("Normal"→"Normaal") · high
- drive / disk → schijf · macOS Finder ("schijf" throughout) · high
- startup disk → opstartschijf · macOS ("Startup Disk"→"Opstartschijf") · high
- restart → Herstart · macOS ("Restart"→"Herstart") · high
- preview → Voorvertoning · macOS ("Preview"→"Voorvertoning") · high
- System Settings → Systeeminstellingen · macOS Finder · high
- Privacy & Security → Privacy en beveiliging · macOS SystemSettings · high
- notification → melding · MS ("notification"→"melding") · high
- port → poort · MS ("port"→"poort") · high
- warning → waarschuwing · MS ("warning"→"waarschuwing") · high
- enable → inschakelen · MS ("enable"→"inschakelen") · high
- disable → uitschakelen · MS ("disable"→"uitschakelen") · high
- word wrap → tekstterugloop · MS ("word wrap"→"tekstterugloop") · high
- network share → netwerkshare · MS ("network share"→"netwerkshare") · high
- share (SMB folder on a server) → share · MS "netwerkshare"; the bare "share" follows for the per-server folder list ·
  high
- cache → cache · MS (unchanged) · high
- shortcut (keyboard) → sneltoets · common macOS/NL UI term; MS "toetsencombinatie" is the longer form · high
- threshold → drempel(waarde) · standard NL · tentative
- provider (AI) → aanbieder · MS first hit "synchronisatievoorziening" is the sync-specific sense (wrong); general
  "aanbieder" · tentative
- service (AI/cloud) → service · MS ("service"→"service", unchanged) · high
- token (AI) → token · standard AI term, kept · high
- context window (AI) → contextvenster · compositional from "venster"; no source term · tentative
- verbose (logging) → uitgebreid · standard NL ("verbose output"→"uitgebreide uitvoer") · tentative

Settings section / card names (keep consistent across files referencing them):

- Appearance → Weergave · macOS uses "Weergave" for view/appearance · high
- Behavior → Gedrag · standard NL · high
- File operations → Bestandsbewerkingen · macOS "bewerking" for operation · high
- File system watching → Bestandssysteem volgen · compositional · tentative
- Search → Zoeken · high
- File systems → Bestandssystemen · high
- SMB/Network shares → SMB-/netwerkshares · high
- MTP (Android/Kindle/cameras) → MTP (Android/Kindle/camera's) · high
- Viewer → Weergavevenster · tentative
- Developer → Ontwikkelaar · high
- Logging → Logboek · high
- Updates & privacy → Updates en privacy · high
- Advanced → Geavanceerd · macOS ("Advanced"→"Geavanceerd") · high
- Keyboard shortcuts → Sneltoetsen · high
- License → Licentie · high
- Colors and formats → Kleuren en notaties · high
- Zoom and density → Zoom en dichtheid · high
- File and folder sizes → Bestands- en mapgroottes · high
- Listing → Lijst · tentative

From the `errors.json` pass (mined `_ignored/i18n/nl/macOS`, 2026-06-21; mostly reuses terms above):

- memory (RAM) → geheugen · macOS ("onvoldoende geheugen beschikbaar") · high
- permission(s) → bevoegdheid / bevoegdheden (access → toegang) · macOS ("geen bevoegdheden", "geen toegang") · high
- quota → quotum · macOS ("quotumlimiet bereikt") · high
- not supported → niet ondersteund · macOS ("worden niet ondersteund op het doelvolume") · high
- unexpected → onverwacht(e) · macOS ("een onverwachte fout") · high
- "couldn't/can't" titles → "Kon … niet" / "Kan … niet"; avoid bare "fout"/"mislukt" as a label per Cmdr voice (macOS
  uses "fout" freely; Cmdr is stricter) · high
- "X failed" titles/toasts → "X lukte niet" · the catalog-wide rendering for "failed" (e.g. "Inloggen lukte niet",
  "Gedeelde map aankoppelen lukte niet", "{volumeName} uitwerpen lukte niet"). The four `errors.write.*.title` "{Verb}
  failed" use "{Verb} lukte niet" to match this (NOT "mislukte", which reads as a bare failure label the voice rule
  discourages). "fout"/"mislukt" are fine only as descriptive words in flowing prose ("dat mislukt meestal", "wanneer er
  een fout optreedt"), never as an error label · high
- "Error: {message}" prefix → "Probleem: {message}" · both `updates.checkToast.errorPrefix` and
  `settings.updates.errorPrefix` use "Probleem:" (the latter was "Fout:", reconciled). Cmdr voice keeps the prefix calm
  · high
- disk image → schijfkopie · macOS · high
- symbolic link → symbolische koppeling · macOS ("koppeling") · high
- alleen-lezen compounds → alleen-lezenvolume, alleen-lezenapparaat (no space) · macOS ("alleen-lezenvolume") · high
- cloud mount (cmVolumes.displayName) → Cloudkoppeling · compositional ("cloud" + macOS "koppeling") · tentative
- "your cloud provider" (genericCloudStorage.displayName) → je cloudprovider · informal `je` per style · high
- provider display/app names (Dropbox, Google Drive, OneDrive, macFUSE, iCloud Drive, …) → kept verbatim (brand names,
  do-not-translate) · confirmed
- OS pane names arrive via `{system_settings}` / `{privacy_and_security}` / `{files_and_folders}` / `{full_disk_access}`
  placeholders (keep the `{token}` literal); the git-error suggestions instead hardcode the English pane names ("System
  Settings > Privacy & Security > Files and Folders"), kept verbatim per their `@key` notes. macOS-feature literals
  "Disk Utility", "First Aid", "Activity Monitor", "Spotlight", "Terminal", "Finder", "Get Info" stay English
  (do-not-translate).

From the `onboarding.json` + `fileOperations.json` pass (mined `_ignored/i18n/nl/`, 2026-06-21):

- trash (noun) → prullenmand · macOS Finder ("Trash"→"Prullenmand", "to the Trash"→"naar de prullenmand") · high
- move to trash → naar prullenmand (button) / naar de prullenmand verplaatsen · macOS Finder · high
- delete (permanently) → definitief verwijderen · macOS uses "definitief"; "Verwijder" for the verb (glossary above) ·
  high
- overwrite → Overschrijf (button) / overschrijven · macOS ("Overschrijf"), Double Commander
  ("Overschrijven"/"Overschrijf alles") · high
- skip → Sla over (button) / Sla alles over · Nautilus ("Overslaan"); rendered as bare-stem imperative "Sla over" per
  the button rule · high
- merge → samenvoegen / samengevoegd · Nautilus ("Merge"→"Samenvoegen", "Map ‘%s’ samenvoegen?") · high
- conflict → conflict · standard NL (no native macOS term mined; "Op deze plaats bevindt zich al …" is macOS's phrasing)
  · high
- "already exists" → "bestaat al" / "Er bestaat al … op deze locatie" · macOS Finder ("Er bestaat al een onderdeel … op
  deze locatie") · high
- destination → bestemming / doelmap · macOS Finder ("destination folder"→"doelmap", "at Destination"→"op bestemming") ·
  high
- rollback → terugdraaien · standard NL (macOS has no exact term); "terugdraaien" is the natural undo-in-progress verb ·
  tentative
- rename (conflict action) → Wijzig naam / Wijzig alle namen · macOS Finder ("Rename"→"Wijzig naam", glossary above) ·
  high
- scanning (progress stage) → Doorzoeken · Double Commander ("Scanning"→"Doorzoeken") · high
- verifying / checking (before op) → Controleren · Double Commander ("Controleer …") · high
- "-ing" progress titles → "Bezig met …" ("Bezig met kopiëren/verplaatsen/verwijderen/annuleren/terugdraaien") ·
  standard NL progress phrasing; keeps the active feel without a clumsy bare gerund · high
- symlink (in copy/delete copy) → symbolische koppeling · matches errors.json "symbolische koppeling"; "target"→"doel" ·
  high
- onboarding (kept) → onboarding · loanword kept verbatim across the file (matches the untranslated app-title key "Cmdr
  onboarding") · tentative
- deny → Weiger · macOS AppKit ("Deny"→"Weiger") · high
- next / back (wizard) → Volgende / Vorige · macOS ("Next"→"Volgende"); "Vorige" is the standard NL pair · high
- finish (wizard) → Voltooi · standard NL wizard button (bare-stem imperative) · tentative
- done → Gereed · macOS AppKit ("Done"→"Gereed") · high
- recommended → aanbevolen · standard NL · high
- provider (cloud AI) → aanbieder · matches settings.json glossary above · high
- API key → API-sleutel · standard NL compound · high
- model (AI) → model · kept · high
- Keychain (macOS credential store) → Sleutelhanger · macOS Dutch · high — Apple FEATURE name Apple localizes per-OS
  (same Decision-1 principle as Quick Look), so use the localized term, NOT the English "Keychain". This SUPERSEDES the
  old "keep Keychain verbatim" rule; "Keychain" is NOT on the don't-translate brand list. The store noun is
  "Sleutelhanger" (app name "Sleutelhangertoegang" below). Apple's Finder/AppKit/SystemSettings pile dump lacks the
  Keychain Access strings, but "Sleutelhanger" is Apple's established Dutch macOS term, and Microsoft's "Windows
  Sleutelhanger" independently confirms it.
- Keychain Access (macOS app) → Sleutelhangertoegang · macOS Dutch · high — Apple's Dutch name for the Keychain
  Access.app; use it verbatim as the app label.
- (system) keyring (non-macOS credential store) → sleutelhanger · same Dutch noun macOS uses; the "Passwords / Keyrings
  app" generic gets "Wachtwoorden / Sleutelhangers" · tentative
- null character → null-teken · MS ("null character"→"null-teken") · high
- absolute path → absoluut pad · MS ("absolute path"→"absoluut pad"); "Pad moet absoluut zijn (begint met /)" · high
- usage statistics → gebruiksstatistieken · standard NL compound · high
- feedback → feedback · loanword kept (standard NL UI) · high
- notification (toast) → melding · matches settings.json glossary above · high

From the `queryUi.json` + `commands.json` pass (mined `_ignored/i18n/nl/`, 2026-06-21):

- paste → Plak (button) / plakken · macOS AppKit MenuCommands ("Paste"→"Plak") · high
- cut → Knip (button) / knippen · macOS AppKit ("Cut"→"Knip") · high
- clipboard → klembord · macOS AppKit ("Clipboard"→"Klembord") · high
- select all → Selecteer alles · macOS AppKit ("Select All"→"Selecteer alles"), Double Commander · high
- deselect all → Deselecteer alles · Double Commander ("Deselecteer alles") · high
- hide (app) → Verberg · macOS Finder ("Hide Finder"→"Verberg Finder") · high
- hide others → Verberg andere · macOS Finder ("Verberg andere") · high
- show all (app menu) → Toon alles · macOS Finder ("Toon alles") · high
- quit (app) → Stop · macOS Finder ("Stop Finder"); macOS uses "Stop" for Quit, NOT "Afsluiten" · high
- about (app) → Over · macOS Finder ("Over Finder"→"Over Cmdr") · high
- Get Info (mac) → Toon info · macOS Finder ("Get Info"→"Toon info") · high
- Quick Look → snelle weergave · macOS Dutch · high — Apple FEATURE name that Apple localizes per-OS, so use the term
  the user sees in their Dutch Finder, NOT the English "Quick Look". Apple's Dutch macOS has no fixed proper-noun: the
  feature noun is "snelle weergave" (AppKit "Close Quick Look"→"Sluit snelle weergave", Finder N169.20 same), and the
  menu-action verb is "Geef snel weer" (Finder TL14, imperative, takes an object: "Geef '^1' snel weer"). Use "snelle
  weergave" for the noun (what Cmdr's strings reference), "Geef snel weer" for an action label.
- context menu → contextmenu · Double Commander ("Toon contextmenu"); MS first hit "snelmenu" is the Windows term,
  contextmenu is the Mac/standard form · high
- zoom in / out → Zoom in / Zoom uit · macOS phrasing ("in- of uitzoomen"); button form "Zoom in/uit" (Zoom in stays
  identical to EN) · high
- zoom to N% → Zoom naar N% · compositional from macOS "Zoom" · high
- extension (file) → extensie · macOS ("extensie", "bestandsextensie"); DC uses "achtervoegsel" but macOS extensie wins
  · high
- sort ascending / descending → Sorteer oplopend / Sorteer aflopend · standard NL UI; "Sorteer op" prefix from glossary
  · high
- pin (tab) → vastzetten · MS ("pin"→"vastmaken"); "vastzetten" reads better for a tab that stays open · tentative
- command palette → opdrachtenpalet · compositional ("opdracht" + "palet"); no source term, matches Cmdr's named UI ·
  tentative
- onboarding → onboarding · loanword kept (matches onboarding.json pass; "Onboarding…" command label,
  "onboardingwizard") · tentative
- offline → offline · MS ("offline", NLD/BEL); "offline beschikbaar" for "available offline" · high
- download (noun/verb) → download / downloaden · MS ("Download", NLD/BEL) · high
- go back / forward (history) → Ga terug / Ga vooruit · standard NL nav (macOS uses Terug/Vooruit) · high
- parent folder → bovenliggende map · macOS Finder, Double Commander ("Ga naar bovenliggende map") · high
- page up / down → Pagina omhoog / Pagina omlaag · standard NL · high
- scroll → schuiven · MS ("scroll"→"schuiven") · tentative
- toggle (X aan/uit) → "X aan/uit" · standard NL toggle phrasing (e.g. "Verborgen bestanden aan/uit") · tentative
- view mode: Brief / Full → Beknopte weergave / Volledige weergave · compositional ("Weergave" from glossary +
  beknopt/volledig) · tentative
- switcher (volume/location) → wisselaar · compositional from "wissel" (no source term) · tentative
- properties (file, non-mac) → eigenschappen · standard NL ("Bestandseigenschappen") · high
- license key → licentiesleutel · NL compound (licentie + sleutel) · high
- upgrade page → upgradepagina · loan "upgrade" + "pagina" · tentative
- query / search query → zoekopdracht · macOS Finder ("zoekopdracht") · high
- index (drive index) → index / Schijfindex · matches fileExplorer "index"; "Schijfindex" compounds with macOS "schijf"
  · high
- scanning (status) → Bezig met scannen · standard NL progress phrasing (cf. fileOperations "Bezig met …") · high
- glob → Glob · technical term kept (no Dutch equivalent) · high
- case-sensitive → hoofdlettergevoelig · standard NL · high
- scope (search) → bereik · standard NL · tentative
- comparator → vergelijkingsteken · standard NL (math comparison sign) · tentative
- "boring folders" (playful) → saaie mappen · literal, keeps the playful product voice per @key · tentative

From the `licensing.json` + `ai.json` + `viewer.json` pass (mined `_ignored/i18n/nl/`, 2026-06-21):

- viewer (read-only file viewer) → weergavevenster · compositional ("weergave" from glossary + "venster"); matches the
  Settings "Viewer" section name (glossary above) · tentative
- About (dialog) → Over · macOS Finder ("Over Finder"→"Over Cmdr"); glossary above · high
- Got it (ack button) → Begrepen · macOS ("Begrepen") · high
- Apply (button) → Pas toe · macOS ("Pas toe") · high
- Continue (button) → Ga door · macOS ("Ga door") · high
- Activate / activating → Activeer (button) / Activeren · macOS ("Activeer") · high
- renew → vernieuwen / Vernieuw (button) · MS ("renew"→"vernieuwen") · high
- perpetual (license) → eeuwigdurend · standard NL legal/license term (no source); "Eeuwigdurende commerciële licentie"
  · tentative
- commercial / personal (license tiers) → commercieel / Personal · "commercieel" translated; tier proper-noun "Personal"
  kept (matches the capitalized EN tier label) · tentative
- valid until / validity → geldig tot / geldigheid · standard NL · high
- expired / expired on → verlopen / Verlopen op · standard NL · high
- clipboard → klembord · MS ("clipboard"→"klembord") · high
- encoding (character) → codering · MS ("encoding"→"codering") · high
- reload (file) → Laad opnieuw (button) / opnieuw laden · MS ("reload"→"opnieuw laden"); bare-stem imperative for the
  button · high
- match (search result) → resultaat · "No matches"→"Geen resultaten", "Next/Previous match"→"Volgend/Vorig resultaat"
  (matches Finder "zoekresultaten" glossary) · high
- word wrap (badge/hint) → terugloop · short form of MS "tekstterugloop" for the terse status badge · tentative
- streaming (viewer mode) → streamen / streammodus · loanword kept (no Dutch UI equivalent for the streaming-read mode)
  · tentative
- tail (follow file, like `tail -f`) → Tail · technical term kept verbatim (no Dutch equivalent); aria/tooltip explain
  it ("volg bestandswijzigingen") · tentative
- Endpoint (API) → Endpoint · technical API term kept; MS literal "eindpunt" not used for an API URL field · tentative
- completions (AI) → completions · loanword kept (AI-API term, no settled Dutch) · tentative
- Stop server / Start server / Download model → identical to EN; all words valid NL (Stop/Start/Server/Download/model
  are standard NL UI terms), so left unchanged · high
- line (of text) → regel · MS ("line"→"regel"); plural "regels" · high
- character (of text) → teken · MS ("character"→"teken"); plural "tekens" · high

From the wave-1 prep pass
(search/feedback/crashReporter/goToPath/transfer/updates/lowDiskSpace/commandPalette/whatsNew/main/common/notifications;
mined `_ignored/i18n/nl/`, 2026-06-21):

- close → Sluit (button) / sluiten · macOS Finder ("Close"→"Sluit", key FR26); same form as "dismiss" → Sluit (glossary
  above) · high
- send → Stuur (button) / versturen · macOS Finder ("Send"→"Verstuur"); chose "Stuur" (shorter imperative, parallel to
  macOS pattern) for "Send feedback"→"Stuur feedback", "Send report"→"Stuur rapport" · high
- remove from list → Verwijder uit lijst · macOS Finder pattern ("Verwijder uit navigatiekolom"/"Verwijder uit
  bibliotheek") · high
- path → pad · standard NL; "Ga naar pad" (macOS Finder "Ga naar map"); "~/Documents" sample kept verbatim · high
- go to path → Ga naar pad · macOS Finder "Ga naar …" nav pattern · high
- feedback → feedback · loanword kept (matches onboarding pass) · high
- note (user's message) → bericht · "Your note"→"je bericht"; "note" as a written message renders as "bericht" · high
- counter ("N / M" chars) → pure placeholders, left "{currentText} / {maxText}" · high
- crash report → crashrapport · matches errors/style glossary; "Report ID"→"Rapport-ID", "report
  details"→"rapportdetails" · high
- error report → foutrapport · NL compound (fout + rapport); used for the update-check "Send error report"→"Stuur
  foutrapport" button · high
- update (noun/verb) → update / bijwerken · MS ("update"); "Restart to update"→"Herstart om bij te werken", "No updates
  found"→"Geen updates gevonden" · high
- restart → Herstart · macOS (glossary above) · high
- later (dismiss button) → Later · same word in NL, left identical · high
- downloading / installing → wordt gedownload / wordt geïnstalleerd · standard NL passive progress; "Download"→download
  (loan, MS NLD/BEL) · high
- running low on space → raakt vol · natural NL for a disk filling up; "low disk space"→"weinig schijfruimte"; "startup
  disk"→"opstartschijf" (glossary above) · high
- free (space) → vrij · standard NL ("{freeText} vrij") · high
- command (palette) → opdracht (plural opdrachten) · DC ("command line"→"opdrachtregel"); matches "opdrachtenpalet";
  "Search commands"→"Zoek opdrachten" · high
- changelog → changelog · loanword kept (casual EN voice; MS "wijzigingenlogboek" is heavier and less common in NL
  software UI) · tentative
- "What''s new" → "Wat is er nieuw" · standard NL ("Wat is er nieuw in Cmdr") · high
- complete (operation done) → voltooid · "Copy/Move/Delete complete"→"… voltooid"; macOS uses "voltooid" for completed
  ops · high
- skipped → overgeslagen · matches fileOperations "Sla over"/"Overslaan" (glossary above); past participle
  "overgeslagen" · high
- "at the target" (destination) → op de bestemming · "destination"→bestemming (glossary above) · high
- onboarding options → onboardingopties · compound of "onboarding" (loan, glossary) + "opties" · tentative
- Full Disk Access → volledige schijftoegang · NL descriptive ("volledige schijftoegang"); the System Settings pane
  label per Dutch macOS · high

REVIEW FLAGS (wave-1 prep pass):

- `transfer.split.clean`/`.skipped` reorder the verb to a sentence-final past participle ("{phrase}
  gekopieerd/verplaatst") — natural Dutch word order vs EN's leading "Copied/Moved {phrase}". The `{phrase}` token stays
  in the same grammatical slot.

REVIEW FLAGS (queryUi/commands pass):

- `queryUi.mode.ai.label` "Ask anything" → "Vraag het maar" (casual, friendly; matches the playful EN). Subjective tone
  call.
- `commands.tabTogglePin.label` pin → "vastzetten" (vs MS "vastmaken"); chose the tab-stays-open sense. Confirm if a
  Dutch macOS/Safari term is preferred.
- `commands.appCommandPalette.label` "opdrachtenpalet" → no source term; coined from "opdracht"+"palet". Confirm it
  reads natural.
- `commands.handler.zoomResetHintMenu` menu path → "Weergave > Zoom > 100%" (translated "View"→"Weergave" to match the
  Dutch menu bar; "Zoom" submenu kept). Verify once the menu bar is translated.

REVIEW FLAGS (onboarding/fileOperations pass):

- "Quit & Reopen" (`onboarding.stepFda.step3`): macOS shows this exact button when relaunching for FDA. Not in the mined
  pile (no `<tag>/macOS` string captured); rendered as "Stop en open opnieuw", the standard macOS-NL FDA button label.
  Confirm against a live Dutch macOS.
- "super private" (`onboarding.stepAi.local.label`): rendered "supergeprivacyd" reads playful/informal to match the
  casual EN ("super private"); a more neutral alternative is "extra privacy wil". Flagged as a tone/subjective call.
- "Pro/Con" (`onboarding.stepFda.pro/con`): rendered "Voordeel/Nadeel" (full words) rather than loan "Pro/Con", which
  reads cleaner in Dutch.

From the `indexing.json` + `downloads.json` + `errorReporter.json` + `shortcuts.json` + `mtp.json` + `ui.json` pass
(mined `_ignored/i18n/nl/`, 2026-06-21):

- scan (drive) → doorzoeken · matches queryUi "Doorzoeken" (Double Commander "Scanning"); "Je schijf wordt doorzocht..."
  for the progress heading · high
- entries (scanned items) → onderdelen · matches fileExplorer "onderdelen" (macOS Finder "items") · high
- events (filesystem changes) → wijzigingen · "wijzigingen verwerkt" reads clearer than literal "gebeurtenissen" ·
  tentative
- left (time remaining) → nog … · "nog 45s" / "nog 3m"; reads natural for a countdown (vs literal "resterend") ·
  tentative
- almost done → Bijna klaar · standard NL · high
- watcher (file change watcher) → bewaker · "bewaker voor bestandswijzigingen", "bewakerskanaal"; from "bewaken" ·
  tentative
- disconnected (drive) → losgekoppeld · macOS "koppel" family (cf. aankoppelen glossary) · high
- outdated / out of date (index) → verouderd · standard NL · high
- jump to (download) → springen naar / Spring naar · natural NL for the quick-jump action · tentative
- global (shortcut scope) → globaal / Globaal · standard NL for system-wide hotkey scope · high
- in-app → in de app · descriptive; no single-word NL term · high
- modifier (key) → wijzigingstoets · macOS/standard NL term for ⌘⌃⌥⇧ · high
- register (a global hotkey) → registreren / geregistreerd · MS; "Registreren lukte niet" avoids bare "mislukt" per Cmdr
  voice · high
- Full Disk Access → Volledige schijftoegang · macOS Privacy pane name · high
- redact / scrub (logs) → schonen · "lokaal geschoond", "na schonen"; natural NL for privacy-redaction · tentative
- reference ID → referentie-ID · NL compound; "ID" kept · high
- note (free-text) → notitie · MS ("note"→"notitie") · high
- manifest → Manifest · technical term kept (identical EN) · high
- main window → Hoofdvenster · compositional ("hoofd" + macOS "venster") · high
- reserved (by macOS) → gereserveerd · standard NL · high
- fixed key → vaste toets / Vast (badge) · standard NL · high
- bound (shortcut) → toegewezen · standard NL ("toewijzen") · high
- Force Quit → Forceer stop · macOS Dutch ("Forceer stop…") · high
- Character Viewer → Emoji''s en symbolen · macOS Dutch (the picker is labeled "Emoji''s en symbolen") · high
- Mission Control / Spotlight / Spaces → kept verbatim · macOS Dutch keeps these proper-noun feature names · high
- app switcher → app-wisselaar · compositional (cf. queryUi "wisselaar") · tentative
- input source → invoerbron · macOS term · high
- screen recording → schermopname · standard NL · high
- screenshots → schermafbeeldingen · macOS Dutch ("schermafbeelding") · high
- device (MTP/USB) → apparaat · macOS Dutch · high
- daemon (system) → daemon · technical term kept; "systeemdaemon", "cameradaemon" · high
- process → proces · standard NL · high
- exclusive access → exclusieve toegang · standard NL · high
- in use (by) → in gebruik (door) · macOS Dutch ("in gebruik") · high
- udev rules → udev-regels · Linux term "udev" kept, "regels" translated · high
- command (terminal) → opdracht · MS ("command"→"opdracht"); matches queryUi · high
- options (popover aria) → Opties · macOS ("Options"→"Opties") · high
- select (dropdown placeholder) → Kies... · macOS picker-prompt sense (vs "Selecteer" for select-all); settled,
  `ui.select.placeholder` is the canonical instance · high
- suggestions (combobox) → suggesties · standard NL · high
- opening (folder) → Map openen... · terse loading line, "Bezig met …" phrasing dropped here · tentative
- dismiss (toast aria) → Sluit melding · "melding" from settings.json glossary · high

REVIEW FLAGS (indexing/downloads/errorReporter/shortcuts/mtp/ui pass):

- `errorReporter.dialog.title` "Send error report" → "Foutrapport versturen" (artifact-noun "foutrapport"; matches the
  existing error-report glossary row). Uses "fout" in a descriptive compound, not as a bare failure label.
- `indexing.replay.detail` "events processed" → "wijzigingen verwerkt" (user-friendly over literal "gebeurtenissen").
  Subjective.
- `ui.select.placeholder` "Select..." → "Kies..." (macOS picker-prompt sense). Confirm vs "Selecteer...".
- `downloads.toast.learnIntro` → "Iets leuks om te leren over snel naar je downloads springen" (kept the playful EN
  tone). Subjective.

REVIEW FLAG (code limitation, out of scope for data-only work): `errors.write.*` strings embed `{verb}` / `{Verb}` /
`{gerund}` placeholders that the frontend substitutes with ENGLISH literals ("copy", "move", "deleting", …) from a
non-localized `operationVerbMap` in `apps/desktop/src/lib/file-operations/transfer/transfer-error-messages.ts`. So a
Dutch sentence like "Het bestand dat je probeerde te {verb}" renders an English verb inline ("… te copy"). The Dutch
phrasing keeps the token in a grammatically plausible slot, but fully natural Dutch needs that verb map localized (a
code change). Same limitation applies to every language.

From the transfer-queue pass (`queue.json` + new pause/queue keys in `fileOperations.json`/`commands.json`; mined
`_ignored/i18n/nl/`, 2026-06-21):

- pause → Pauzeer (button) / pauzeren, gepauzeerd · macOS Finder ("Kopiëren van '^0' is gepauzeerd", "Wil je ...
  pauzeren"), AppKit ("Pauzeer animatie"), DC ("Pauzeer alles"); bare-stem imperative "Pauzeer" per the button rule ·
  high
- resume → Hervat (button) / hervatten · macOS Finder ("Hervat", "Hervat kopiëren") · high
- queue (noun) → wachtrij (Overdrachtswachtrij for the transfer queue) · Double Commander + Total Commander + Thunar all
  use "wachtrij" (no macOS term); compound "overdrachtswachtrij" for "transfer queue" · high
- transfer (copy/move/delete operation, noun) → overdracht (plural overdrachten) · standard NL ("overdracht" for a data
  transfer); "Transfer queue"→"Overdrachtswachtrij", the queue window title/heading · high
- background (run in the ~) → op de achtergrond · Double Commander ("Werk op de achtergrond"); "send to
  background"→"naar de wachtrij sturen" / "op de achtergrond laten doorlopen" (the action sends it to the queue window)
  · high
- status words (queue rows): Waiting → Wachten · macOS Finder ("Wachten"); Running → Bezig · DC ("Bezig"), matches the
  "Bezig met …" progress family; Done → Gereed · macOS Finder (glossary above); Cancelled → Geannuleerd · macOS Finder
  ("Geannuleerd"); "Couldn''t finish" (gentle failed) → Niet voltooid · macOS uses "kon niet worden voltooid"; short
  status "Niet voltooid" avoids a bare "mislukt" label per Cmdr voice · high

REVIEW FLAGS (transfer-queue pass):

- `queue.row.label` reuses the "Bezig met …" progress phrasing (kopiëren/verplaatsen/verwijderen) from fileOperations
  `titleActive`/`stageActive`, with the trash branch "Naar prullenmand verplaatsen" matching there. Consistent across
  files.
- `fileOperations.transferProgress.queuedToast` + `.queuedToastCount`: the EN puts the count phrase ("1 transfer")
  leading; Dutch needs the verb to agree, so the count phrase carries it ("gaat # overdracht" / "gaan # overdrachten")
  and the host sentence wraps it as "Er {countText} deze voor, dus deze wacht op zijn beurt." Renders "Er gaat 1
  overdracht deze voor" / "Er gaan 3 overdrachten deze voor". Token kept in the same slot.

From the navigation + double-click-hint pass (`settings.json` Behavior restructure + new
`fileExplorer.doubleClickHint.*` and `breadcrumb.navigateTooltip`; first drafted glossary-only, then RE-VALIDATED
against the reference pile `_ignored/i18n/nl/`, 2026-06-26):

- rename (in the section summary) → naam wijzigen, NOT hernoemen · macOS Finder uses "Wijzig naam" / "naam wijzigen"
  exclusively (key cross-ref `nl/macOS/Finder/LocalizableMerged.json`: "Rename"→"Wijzig naam", "The item can't be
  renamed"→"De naam ... kan niet worden gewijzigd"); confirms the existing glossary rename row.
  `settings.summary.navigationAndFileOps` changed "het hernoemen van bestanden" → "het wijzigen van bestandsnamen" to
  match · high
- double-click → Dubbelklik (button/imperative) / dubbelklikken (verb, gerund) · Double Commander ("double click for
  files"→"dubbelklik voor bestanden"), KDE Dolphin ("double clicking view background"→"dubbel klikken op ...
  achtergrond"); already used in fileExplorer tooltips · high
- pane background → paneelachtergrond · compound of glossary "paneel" + "achtergrond"; KDE Dolphin renders "view
  background"→"... achtergrond" (background→achtergrond corroborated), pane→paneel from the glossary (Double Commander)
  · high
- navigate (to a path/folder) → naar … gaan · macOS Finder attests both "navigeren naar de bovenliggende map"
  ("Navigates ... to its enclosing folder") and the menu "Ga naar bovenliggende map"; chose the shorter "Ga naar"/"naar
  … gaan" for tooltips ("Klik om naar {path} te gaan"). MS terminology confirms navigate→navigeren if the literal verb
  is ever wanted · high
- "Navigation & file ops" (Settings section, short) → Navigatie en bewerkingen · the short sidebar/page heading;
  "Navigatie" confirmed (MS terminology "Navigation"→"Navigatie", ProperNoun). "bewerkingen" mirrors the EN casual
  shortening of "operations"→"ops"; the sibling card heading keeps the full "Bestandsbewerkingen" (glossary "File
  operations"). "&" → "en" (matches "Updates en privacy", "Privacy en beveiliging") · tentative (the clip, not the term)
- Navigation (card heading) → Navigatie · MS terminology ("Navigation"→"Navigatie") · high
- parent folder → bovenliggende map · reuses glossary row, now doubly corroborated here (macOS Finder "enclosing
  folder"→"bovenliggende map" across many keys; Double Commander "Go to parent directory"→"Ga naar bovenliggende map") ·
  high
- hint (internal seen-flag label) → hint · MS terminology keeps "hint"→"hint" (NLD/BEL); the
  `doubleClickOnPaneNotificationSeen.*` keys are internal/hidden so this is low-stakes · high
- row (file-list row) → rij; "file row" → bestandsrij · MS terminology ("row"→"rij", NLD/BEL), Double Commander ("one
  per row"→"één per rij"). Used in `doubleClickPaneNavigatesToParent.description` ("not a file row"→"geen bestandsrij")
  · high

David later shortened the EN for the double-click setting; the two re-worded values keep the settled terms:

- `doubleClickPaneNavigatesToParent.label` EN "Double-click the pane background to go up a folder" → "Dubbelklik op de
  paneelachtergrond om naar de bovenliggende map te gaan" ("go up a folder" rendered with the settled "bovenliggende
  map").
- `doubleClickPaneNavigatesToParent.description` EN "That''s the empty space around the file list, not a file row." →
  "Dat is de lege ruimte rondom de bestandenlijst, geen bestandsrij." ("around" → "rondom"; "not a file row" → "geen
  bestandsrij"; reuses "lege ruimte" + "bestandenlijst"). No ICU apostrophe needed — "Dat is" has none.

REVIEW FLAGS (navigation/double-click-hint pass):

- `fileExplorer.doubleClickHint.dontLikeIt` "Don''t like it?" → "Bevalt het niet?" (friendly, concise; pairs with the
  "Doe dit nooit meer" / "Ik vind het leuk" buttons). Subjective tone call; pile has no UI-microcopy match for this
  phrase.
- `fileExplorer.doubleClickHint.iLikeIt` "I like it" (primary button) → "Ik vind het leuk". Subjective; alternatives
  "Prima"/"Top!" are shorter but less literal.
- `settings.section.navigationAndFileOps` "Navigatie en bewerkingen": shortened-section vs full-card distinction is a
  deliberate parallel to EN's "ops". Confirm the sidebar width is comfortable; the fuller "Navigatie en
  bestandsbewerkingen" is the fallback if the clip reads odd.

## Cross-file reconciliation (2026-06-21)

Drift the parallel per-file passes left behind, found and fixed in a whole-locale reconciliation. Recorded so the
rulings stay single-sourced and don't drift again:

- **Ellipsis style follows the EN source per key.** EN mixes `…` and `...` deliberately (per key); match it. The
  ai/licensing/settings/viewer passes had silently converted 32 EN `...` to Unicode `…`; reverted to `...` to match EN
  (most files already preserved `...`). Don't normalize ellipses to one glyph — mirror EN.
- **Quoted UI strings inside running text use single curly quotes `‘…’`**, never straight `"…"` (style.md). The
  commands/fileExplorer/settings passes left 15 values with straight `"…"` (EN's quoting); converted to `‘…’`. The
  fileOperations/onboarding/shortcuts passes already used `‘…’`. This is the locale-wide convention now.
- **Brand tokens stay verbatim, so avoid the Dutch genitive-s on them.** `errorReporter.dialog.description` had "Cmdrs
  recente logbestanden" (the `desktop-i18n-dont-translate` check reads "Cmdrs" as a dropped "Cmdr"); rephrased to "de
  recente logbestanden van Cmdr". Prefer the `van X` construction over `X's`/`Xs` for brand names.
- **Settings-section references match across files** (verified): "Instellingen > AI" ↔ `settings.section.ai`;
  "Instellingen > Sneltoetsen" ↔ `settings.section.keyboardShortcuts`; "Instellingen > Updates" (crashReporter) vs
  "Instellingen > Updates en privacy" (whatsNew) are deliberately distinct, each tracking its own EN source and the
  glossary section names. Menu-path separators (`>` vs `→`) mirror EN per key.
- preset (value in a settings-picker dropdown) → voorinstelling; "back to presets" → "Terug naar voorinstellingen" ·
  Double Commander nl ("voorinstelling": "gewijzigde voorinstelling") · high

From the `filesTooLargeForFilesystem` pass (FAT32 file-size-cap error; mined `_ignored/i18n/nl/`, 2026-06-30):

- too large (file vs drive) → te groot · standard NL ("te groot" for size; "te lang" is reserved for over-long names) ·
  high
- format / formatted as (disk) → geformatteerd als · Microsoft terminology ("format"→"formatteren", Verb, NLD/BEL);
  Apple's Disk Utility labels the format field "Structuur:" but the verb form "geformatteerd als FAT32/exFAT" is the
  natural sentence form · high
- store (files on a drive) → bewaren · macOS Finder ("Store … in iCloud"→"Bewaar … in iCloud"); same verb as save
  (glossary save→bewaren, NOT opslaan) · high
- larger than → groter dan · Microsoft terminology ("larger than"→"groter dan") · high
- FAT32 / exFAT (filesystem format names) → kept verbatim · Apple keeps "ExFAT"/"MS-DOS (FAT)" in Dutch macOS;
  filesystem-format names, do-not-translate · high
- limit (file-size cap) → beperking · Microsoft terminology ("restriction/limit"→"beperking"); "die heeft zo'n beperking
  niet" for "has no such limit" · high
- "and N more files" (trailing list line) → "en nog {countText} bestand(en)" · "nog" carries the "more/additional"
  sense; plural one/other → bestand/bestanden · high
- preset (value in a settings-picker dropdown) → voorinstelling; "back to presets" → "Terug naar voorinstellingen" ·
  Double Commander nl ("voorinstelling": "gewijzigde voorinstelling") · high

From the dialog-polish pass (new `fileOperations` keys; mined `_ignored/i18n/nl/`, 2026-06-30):

- action (what a control chooses; screen-reader label `transferDialog.operationAria`) → Bewerking · macOS AppKit ("This
  action cannot be performed"→"Deze bewerking kan niet worden uitgevoerd"); matches glossary "File
  operations"→"Bestandsbewerkingen". macOS "bewerking" (Tier 1) over Double Commander "actie" (DC: "The action cannot be
  completed"→"De actie kan niet worden voltooid") · high
- scanning (spinner tooltip/SR label while counting items, "Scanning…") → Doorzoeken… · Double Commander
  ("Scanning"→"Doorzoeken"); matches the existing `transferProgress.stageScanning` "Doorzoeken" in this file. Ellipsis
  kept · high
- create (a folder) → aanmaken (maakt … aan) · macOS Finder ("Create a folder named …"→"Maak … een map aan", "Could not
  create the folder."→"De map kon niet worden aangemaakt."); matches the existing in-file "dat de map is aangemaakt"
  (mkdir.timeoutMessage). Separable verb: "Cmdr maakt hem aan" · high
- "doesn''t exist yet / will be created" (destination-folder warning) → "Deze map bestaat nog niet. Cmdr maakt hem aan
  tijdens het {kopiëren/verplaatsen}." · negative of macOS Finder "bestaat al" (→"bestaat nog niet"); "hem" pronoun for
  de-word "map" (cf. in-file "Vind hem in de overdrachtswachtrij"); operation verb "het kopiëren"/"het verplaatsen"
  matches in-file scanTitle/scanPhase usage. No ICU apostrophe needed (Dutch has none) · high
- **queue.row.label progress arms (rename / create folder / create file)** · `Bezig met hernoemen` /
  `Bezig met map aanmaken` / `Bezig met bestand aanmaken` · "Bezig met [infinitief]" style of the sibling arms; Nautilus
  ("hernoemen", "aanmaken"), settled `map`/`bestand` · high

From the archive-browsing pass (28 keys across errors/fileExplorer/fileOperations/settings/viewer/queue; mined
`_ignored/i18n/nl/`, 2026-07-05):

- archive (noun, zip/tar/7z browsed like a folder) → archief (plural archieven) · macOS Finder ("Zip
  archive"→"Zip-archief", "Kind is archives"→"Soort is Archief"); already in-catalog (settings.fileViewer "afbeelding,
  PDF, archief …") · high
- archive format → archiefstructuur · macOS Finder ("Zip archive format"→"Zip-archiefstructuur", "Compression format to
  use"→"Te gebruiken compressiestructuur"); the bare "format" for an entry's compression method also renders "structuur"
  (macOS "compressiestructuur") · high
- app bundle (.app/.bundle/.framework, folder shown as one item) → pakket / App-pakketten · macOS Finder ("Show Package
  Contents"→"Toon pakketinhoud" — Apple's user-facing word for a .app is "pakket", Tier 1 over MS "bundel"). ariaLabel
  uses bare "pakket"; the Settings card/row label uses "App-pakketten" (keys 16 & 19 match) · high
- browse (step inside an archive/bundle, list contents like a folder) → bladeren; Blader (short menu/toggle imperative),
  doorbladeren (in a sentence) · macOS Finder ("Browse"→"Blader", key 48.title), MS ("browse"→"bladeren", NLD/BEL).
  Distinct from "open" (hand to default app): "Blader als een map" vs "Open met standaardapp" · high
- extract (unpack an archive) → uitpakken · Double Commander ("Bestanden uitpakken"), MS ("extract"→"uitpakken") · high
- default app → standaardapp · macOS Finder ("no default app specified"→"Er is geen standaardapp opgegeven", N141) ·
  "Open with default app"→"Open met standaardapp" · high
- edit (change a zip's entries) → bewerken · macOS ("bewerking"/glossary File operations→Bestandsbewerkingen), Double
  Commander ("Bewerken"); queue.row.label archive_edit arm → "Bezig met archief bewerken" ("Bezig met [infinitief]"
  family) · high
- damaged → beschadigd · macOS Finder ("corrupt/damaged"→"beschadigd", LA33/NE59) · high
- encrypted → versleuteld · macOS ("Encrypted"→"Versleuteld") · high
- configure → Configureer · macOS Finder ("Configure Time Machine…"→"Configureer Time Machine…", BU3, keeps the
  ellipsis) · high
- ask (segmented-control option, "ask each time") → Vraag · imperative stem of "vragen"; Double Commander ("ask each
  time which to use"→"vraag elke keer welke te gebruiken") · high
- open (segmented-control option / imperative) → Open · macOS Finder uses "Open" as the button/menu imperative ("Open in
  New Window"→"Open in nieuw venster"); coincides with EN, recorded via `sameAsSourceJustification` on
  `settings.archives.opt.open` · high
- for good / permanently (delete has no trash) → definitief · macOS ("definitief verwijderen"; glossary delete
  permanently); "worden definitief uit de zip verwijderd" · high
- read-only archive → Alleen-lezen archief · matches the SIBLING `fileExplorer.readOnly.deviceTitle` "Alleen-lezen
  apparaat" (space form) for local consistency; errors.json prefers the no-space compound "alleen-lezenvolume" —
  divergence flagged · high
- "each format" (generic, in the settings blurb) → elk formaat · in-catalog "formaat" precedent; macOS "structuur" is
  the archive-specific compress form, reserved here for "archiefstructuur"/"structuur" · high
- format-token compounds hyphenate lowercase per macOS ("Zip-archief"): zip-archieven, tar-archieven, 7z-archieven,
  zip-bestand · high

REVIEW FLAGS (archive-browsing pass):

- **app bundle → "App-pakketten"** is the Tier-1 macOS choice ("Toon pakketinhoud" = Show Package Contents, Apple's word
  for a .app), preferred over MS "bundel". Risk: "pakket" can also read as an installer (.pkg); the
  ".app/.bundle/.framework" descriptions disambiguate. Confirm it reads right, or fall back to "App-bundels".
- **read-only archive → "Alleen-lezen archief" (with space)** to match the neighboring `deviceTitle` "Alleen-lezen
  apparaat". This diverges from the errors.json no-space compound rule ("alleen-lezenvolume"). Worth a locale-wide
  decision on which form wins for "alleen-lezen" + noun.
- **preview (verb) → "bekijken"** in `viewer.error.archiveTooLarge` ("om vanuit het archief te bekijken"); macOS
  "Voorvertoning" is the Quick-Look noun, so the plain verb "bekijken" (view) is used. Subjective.

From the paste-clipboard-as-file pass (5 `settings.fileOperations.pasteClipboardAsFile.*` + 2
`fileExplorer.clipboard.pastedAsFile*`; mined `_ignored/i18n/nl/`, 2026-07-07):

- paste (past participle, clipboard content saved as a file) → geplakt · macOS AppKit ("Paste"→"Plak"; PASTEBOARD
  verification substring "plak"), Double Commander ("Plak wat werd geknipt"); glossary paste→Plak/plakken. Toast uses
  the sibling `clipboard.copied` "… gekopieerd" object-then-participle order · high
- clipboard content → klembordinhoud · compound of macOS "Klembord" ("Clipboard"→"Klembord") + "inhoud"; the label
  "Klembordinhoud als bestand plakken" · high
- clipboard image / clipboard text → Klembordafbeelding / Klembordtekst · compounds of macOS "Klembord" + in-catalog
  "afbeelding" (settings.fileViewer "afbeelding, PDF, archief") / "tekst". Hyphenate before the acronym: Klembord-PDF
  (Dutch rule, cf. "SMB-share", "macOS-versie"). The full compound sits INSIDE each select branch so PDF keeps its
  hyphen; the branch is the sentence start so all three capitalize · high
- do nothing → Niets doen · Double Commander ("Do nothing"→"Doe niets"); rendered infinitive "Niets doen" (not
  imperative "Doe niets") to stay parallel with the sibling radio options "Bestand aanmaken" / "Aanmaken en naam
  wijzigen" · high
- create file → Bestand aanmaken · glossary create→aanmaken (macOS Finder "Maak … aan") + macOS "bestand"; radio-option
  infinitive · high
- create and rename → Aanmaken en naam wijzigen · glossary create→aanmaken + rename→naam wijzigen (macOS Finder "Wijzig
  naam" / "naam wijzigen"); radio-option infinitive · high

## Archive-password dialog (2026-07-08)

Terms settled while translating the encrypted-archive unlock modal (`fileOperations.archivePassword.*`; macOS AppKit +
Total/Double Commander nl).

- password-protected → `beveiligd met een wachtwoord` · TC/DC nl phrasing · high. Body: "… is beveiligd met een
  wachtwoord."
- password (noun) → `Wachtwoord` · macOS/MS · high. Input aria-label compounds to `Archiefwachtwoord`.
- unlock (button + verb) → `Ontgrendelen` · macOS AppKit ("Ontgrendelen") · high. Verb "om het te ontgrendelen".
- archive (the `{name}` head) → `archief` · settled nl glossary · high.

Settled while translating the Compress feature:

- compress (verb / control label) → `Comprimeer` (imperative, matching the sibling `Kopieer`/`Verplaats`) · Finder
  `nl/macOS` ("Comprimeer", `Compress ${sources}` → "Comprimeer ${sources}") · high. Used for
  `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and both title-verb branches.
- compressing (progress form) → `Bezig met comprimeren` · derived on `Bezig met kopiëren`/`verplaatsen` · high.
  `scanTitleCompress` = "Controleren voor het comprimeren...".
- compressed (result toast) → `gecomprimeerd` (past participle) · mirrors `transfer.split.clean` ("{phrase} gekopieerd")
  · high.
- replace (overwrite warning) → `vervangt` · Finder `Replace` → "Vervang" · high.
- archive (name) → `archief` · Finder `Zip archive` → "Zip-archief" · high. `.zip` in straight double quotes.
- compression level (slider label) → `Compressieniveau` · TC `nl` "Interne ZIP-compressie (0-9)" + `niveau`; standard nl
  7-Zip `Compressieniveau` · high. `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `Sneller` · TC `nl` "snelste compressie (1)" (root `snel`) · high. Marks quicker
  packing, not app speed. `.faster`.
- smaller (slider high end, level 9) → `Kleiner` · pairs with `Sneller`; marks the smaller output file (TC `nl` high end
  "maximale compressie") · high. `.smaller`.
- No `sameAsSourceJustification` needed: all values differ from English.

From the Operation-log pass (`operationLog.json` + `commands.logOperationLog.*`; mined `_ignored/i18n/nl/`, 2026-07-10):

- operation log (feature name, dialog title + command label) → `Bewerkingenlogboek` · compound of "bewerkingen" (macOS
  operation→bewerking, glossary "File operations"→"Bestandsbewerkingen") + "logboek" (MS "log"→"logboek"; glossary
  Logging→Logboek). Concatenated per the compound rule, with -en- linking like "bestandenlijst" · high
- operation history (in prose, `dialog.loadError` + command description) → `bewerkingsgeschiedenis` / `geschiedenis` ·
  macOS "history"→"geschiedenis" ("version history"→"versiegeschiedenis", NSToolbarHistoryTemplate →"geschiedenis"); the
  compound "bewerkingsgeschiedenis" uses -s- linking like macOS "versiegeschiedenis" · high
- roll back (verb) → `terugdraaien`; "roll them back"→"draai ze terug" (imperative "draai ... terug") · reuses the
  settled fileOperations rollback term ("Bezig met terugdraaien...", "conflictRollback"→"Terugdraaien") · high
- rolled back (past participle, status + per-item outcome) → `Teruggedraaid`; "partly rolled back"→"Gedeeltelijk
  teruggedraaid"; "rolling back"→"Bezig met terugdraaien" (matches fileOperations `titleRollingBack`) · high
- can (not) roll back (rollback-capability status badges) → `Terug te draaien` / `Niet terug te draaien` · the Dutch
  "te + infinitief" -able construction reads as a clean adjectival status pair · high
- rename summary ("Renamed N items") → `Naam van {countText} onderdeel gewijzigd` /
  `Namen van {countText} onderdelen gewijzigd` · honors the strongly-settled rename→"naam wijzigen" (NOT hernoemen;
  macOS Finder "De naam van het onderdeel ... gewijzigd"); reordered in `dialog.empty` so "wijzig de naam van iets"
  keeps its object · high
- lifecycle status words (operation log) reuse queue.row.status: Queued→`Wachten`, Running→`Bezig`, Done→`Gereed`,
  "Didn''t finish"→`Niet voltooid`, Canceled→`Geannuleerd`; per-item Skipped→`Overgeslagen` (glossary) · high
- initiator labels: You→`Jij` (contrastive standalone), AI client→`AI-client` (MS client→client, hyphenated after the
  acronym), Agent→`Agent` (kept, `sameAsSourceJustification`) · high
- recorded (items) → `vastgelegd` · natural NL for logged/recorded ("geen vastgelegde onderdelen") · tentative
- "and N more items" (trailing list line) → `en nog {countText} onderdeel(en)` · matches the FAT32 pass "en nog
  {countText} bestand(en)" pattern ("nog" carries the more/additional sense) · high
- No `sameAsSourceJustification` needed except `initiator.agent` ("Agent").

From the Ask Cmdr pass (`askCmdr.json` full catalog + `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`,
`settings.section.askCmdr`, `commands.askCmdrToggle.*`; mined `_ignored/i18n/nl/`, 2026-07-13):

- chat (the Ask Cmdr conversation feature, noun) → `chat` (plural `chats`) · Microsoft terminology confirms `chat` as a
  native NLD/BEL noun (alongside `chatgesprek`); Cmdr's own UI already names the feature "Chats"
  (`askCmdr.sessions.title`/`askCmdr.threads.open`), matching how mainstream Dutch chat UIs (WhatsApp, Messenger) label
  a conversation list · high. This supersedes Microsoft's generic `session`→`sessie` for this concept: Cmdr calls a
  saved conversation a "chat" throughout (`newChat`, `sessions.rename`, …), so `sessie` is reserved for other, unrelated
  technical "session" concepts, not this one.
- thinking (AI reasoning status, `askCmdr.thinking`) → `Nadenken…` · bare infinitive + ellipsis, following the
  established "'-ing' progress titles → bare infinitive" convention already used for single-word progress
  (`Doorzoeken…`, `Verbinden…`) · tentative (no AI-assistant precedent in the pile; Microsoft's dictionary entry for
  "thinking" is a mistranslated ProperNoun sense, not usable).
- tool (an AI tool call, `askCmdr.tool.*`) → `hulpmiddel` · Microsoft terminology ("tool"→"hulpmiddel") · high
- attachment (a file/folder attached to a question) → `bijlage` · Microsoft terminology ("attachment"→"bijlage") · high
- attach (verb, attach a file/folder to a question) → `bijvoegen` · paired with the settled noun `bijlage` (same root,
  as in "een bijlage bijvoegen aan een e-mail"); Microsoft's "attach"→"beschikbaar maken" is the wrong sense
  (device/service attach, not a file attachment) · tentative
- archive / unarchive (hide or restore a chat from the active list, Gmail-style — NOT the zip/compress sense) →
  `Archiveer` (button, bare-stem imperative) / `Uit archief halen`; archived (badge/adjective) → `gearchiveerd` · no
  pile source for this sense (Nautilus/Total Commander "archive" is compression, a different concept per the
  four-gotchas rule); coined from the settled noun `archief` · tentative. `Uit archief halen` is a full verb phrase, not
  a single-word imperative, for lack of a natural single Dutch reverse-of-archiveren verb.
- (tool-step or time) budget / limit (`askCmdr.error.budgetExhausted`) → `limiet` · reuses the general NL word for a
  cap, distinct from the FAT32-specific `beperking` (glossary above); the literal word "budget" never appears in the
  rendered NL string · tentative
- estimate (AI cost estimate, `settings.askCmdr.spend.disclaimer`) → `schatting` · NOT Microsoft's first hit "offerte"
  (that's the business-quote sense — a mining trap-4 wrong sense); "schatting" is the plain generic sense · high
- dashboard (provider's billing dashboard) → `dashboard` · Microsoft terminology (unchanged loanword) · high
- spending (`settings.askCmdr.spend.title`) → `Uitgaven` · Microsoft terminology ("spending"→"uitgaven") · high
- usage (token/AI usage) → `gebruik` · Microsoft terminology ("usage"→"gebruik") · high
- on-device (cost readout "free, on-device") → `lokaal` · concise for the terse lowercase cost readout; matches Cmdr's
  on-device/local-model framing elsewhere · tentative
- Brand + possessive ("Cmdr's other AI features", "Cmdr's AI") → rephrase with the settled `van Cmdr` construction
  (`de andere AI-functies van Cmdr`, `de AI van Cmdr`), reapplying the cross-file-reconciliation rule against a
  dropped-brand genitive-s (`errorReporter.dialog.description` precedent above) · high
- Ask Cmdr + suffix (`settings.askCmdr.interactiveModel.label` "Ask Cmdr model") → `Ask Cmdr-model` · hyphenates after
  the full two-word brand name, same shape as the existing brand+hyphen+noun pattern (`macOS-versie`,
  `SMB-/netwerkshares`) · tentative
- `askCmdr.cost.tokens` ICU plural string renders byte-identical to English (`sameAsSourceJustification` recorded):
  Dutch CLDR has the same one/other categories as English, and `token`/`tokens` is the settled kept AI loanword
  (glossary above) · high

REVIEW FLAGS (Ask Cmdr pass):

- The seven AI tool-status `doing`/`done` pairs
  (`askCmdr.tool.appState/listDir/largestDirs/importantFolders/ folderImportance/listVolumes/operationsList/operationsGet`)
  have no reference-pile precedent — these are AI-assistant tool-call status lines, a domain none of the five file
  managers or macOS/Microsoft cover. Rendered as present-tense-no-subject for `doing` (e.g. "Controleert wat je
  bekijkt") and past-participle-led for `done` (e.g. "Bekeken wat je bekijkt", "Grootste mappen gevonden"), picking a
  distinct verb per tool so the seven pairs stay disambiguated. Subjective/tentative as a set; flagged for native review
  if one becomes available.
- `askCmdr.sessions.unarchive` "Uit archief halen": no single-word Dutch imperative exists for "unarchive" the way
  `Archiveer` does for "archive". Confirm this reads acceptably next to the shorter sibling buttons, or shorten if a
  better idiom turns up.
- `askCmdr.composer.dropHint` "Drop to attach" → "Zet hier neer om bij te voegen": no pile source for a drag-and-drop
  invitation overlay; phrased from the settled `bijvoegen` verb. Subjective.

From the network image-indexing pass (`settings.mediaIndex.networkVolumes.*` + `settings.mediaIndex.alwaysIndex*` + 2
`search.imageResults.*`; mined `_ignored/i18n/nl/`, 2026-07-13):

- network drive → `netwerkschijf` (plural `netwerkschijven`) · macOS Finder (`nl/macOS`: "Netwerkschijf koppelen…",
  "Verbreek verbinding met netwerkschijf…") + glossary drive→`schijf`; Tier 1, preferred over Windows-flavoured
  "netwerkstation" · high
- photo → `foto` (plural `foto''s`, ICU-doubled apostrophe) · macOS ("Foto''s" app, "^0 foto''s ontvangen") · high.
  Mirrors the EN source's deliberate register split: internal/technical labels keep `image`→`afbeelding` (matches the
  already-translated `settings.mediaIndex.card` "Afbeeldingen doorzoeken" / `enabled.label` "Inhoud van afbeeldingen
  indexeren"), while the warm user-facing network-drive rows say `foto''s` (the network case is a photo archive/NAS).
- image (technical/label sense) → `afbeelding` (plural `afbeeldingen`) · macOS ("Afbeelding"/"Afbeeldingen") · high.
  "Image indexing" (internal label + the search hint) → "afbeeldingen indexeren" / "het indexeren van afbeeldingen".
- indexed → `geïndexeerd` (past participle) · glossary index→indexeren; "Not indexed yet"→"Nog niet geïndexeerd", "N
  photos indexed"→"{countText} foto''s geïndexeerd" · high
- reconnect (drive comes back) → `opnieuw verbinding maken` · macOS Finder ("opnieuw verbinding maken met '^0'");
  "resumes when this drive reconnects"→"gaat verder zodra deze schijf opnieuw verbinding maakt" · high
- disconnect / disconnected (drive drops off mid-pass) → `wordt losgekoppeld` / `is losgekoppeld` · reuses the
  indexing-pass glossary disconnected→`losgekoppeld` (macOS koppel-family) · high
- background indexing → `achtergrondindexering` · compound of "achtergrond" (glossary "op de achtergrond") +
  "indexering" (index→indexeren) · tentative
- photo archive → `fotoarchief` · compound of `foto` + glossary archive→`archief` · high
- resume (auto, of a paused pass) → `gaat verder` · plainer than glossary button `Hervat` for a passive status line
  ("gaat verder zodra …") · high
- No `sameAsSourceJustification` needed: all 19 values differ from English.

From the indexing run-kind + hour-scale-ETA pass (`indexing.run.*`, `indexing.eta.hours*`, `indexing.enrich.queued`,
`settings.mediaIndex.importanceThreshold.waitingForDriveIndex`; 2026-07-18):

- run-kind headers (checklist top): First full scan → `Eerste volledige scan`; Full rescan → `Volledige herscan`; Quick
  update → `Snelle update` · reuses in-catalog "scan" as a noun (`indexing.step.findFilesFirstScan` "Eerste scan, dit
  kan even duren"), "volledig" (glossary "volledige schijftoegang"/"Volledige weergave"), and update→`update` (glossary
  "update/bijwerken"). "herscan" is a compact coinage parallel to "scan"; the rescan toasts spell the verb form as "de
  schijf opnieuw doorzoeken" · high (headers) / tentative (`herscan` noun)
- hour(s) (time-remaining, spelled out) → `uur` in BOTH plural branches · Dutch keeps `uur` invariant after a cardinal
  for durations ("2 uur", "20 uur", not "uren"); macOS shows "uur" for remaining time · high
- minute(s) (time-remaining, spelled out) → `minuut` (one) / `minuten` (other) · standard NL · high
- "… left" (hour-scale ETA) → leads with `nog …` · matches the sibling `indexing.eta.minutesLeft`/`.secondsLeft` ("nog
  {n}m"/"nog {n}s") and the glossary "left (time remaining) → nog …"; renders "nog 1 uur 24 minuten" / "nog 20 uur" ·
  high
- "the drive scan" (as a noun phrase in prose) → `het doorzoeken van de schijf` / `de schijf wordt nog doorzocht` ·
  glossary scan→doorzoeken; `indexing.enrich.queued` = "Het indexeren van afbeeldingen begint na het doorzoeken van de
  schijf", `waitingForDriveIndex` = "De schijf wordt nog doorzocht. …" (parallels the sibling `waitingForImportance`
  "Het indexeren van afbeeldingen begint direct daarna.") · high
- No `sameAsSourceJustification` needed: all 7 values differ from English.

From the quality pass over the bulk-rename / image-index-scope / Ask Cmdr-tool keys (54 keys across
`askCmdr`/`errors`/`fileExplorer`/`settings`; re-mined `_ignored/i18n/nl/`, 2026-07-20):

- rename (noun: one proposed rename, "renames", "rename plan") → `naamwijziging` (plural `naamwijzigingen`), compound
  `naamwijzigingsplan` · Microsoft terminology renders the noun "rename suggestions" → "naamwijzigingsvoorstellen"
  (NLD/BEL), which fixes both the noun `naamwijziging` and its `-s-` link before a following noun; the verb side stays
  the settled `naam wijzigen` (macOS Finder "Rename"→"Wijzig naam", MS "Rename"→"Naam wijzigen") · high. ❌ NOT
  `hernoeming`/`hernoemingsplan`: `hernoemen` has ZERO hits in `nl/macOS/` (Tier 1) and is a Tier-3-only form
  (Nautilus/DC/Dolphin, 78 hits), so it loses to the doubly-corroborated macOS+Microsoft form.
- allow (button) → `Sta toe`; allow all → `Sta alles toe` · macOS Finder ("Allow Anyway"→"Sta toch toe") + MS
  ("allow"→"toestaan"); bare-stem imperative per the button rule, with the separable `toe` sentence-final, and the
  "verb + alles" shape macOS uses ("Selecteer alles", "Vervang alles") · high
- deny (button) → `Weiger`; deny all → `Weiger alles` · macOS AppKit "Weiger" + MS ("deny"→"weigeren"); already the
  in-catalog form at `onboarding.stepFda.deny` · high
- add (button, "Add a folder…") → `Voeg … toe` · macOS Finder ("Add Password"→"Voeg wachtwoord toe", "Add Tags"→"Voeg
  tags toe", "Add"→"Voeg toe") · high. The Settings row button is "Voeg een map toe…".
- remove (button) → `Verwijder` · macOS AppKit ("Remove"→"Verwijder"), Finder ("Remove from Sidebar"→"Verwijder uit
  navigatiekolom"); matches the in-catalog `fileExplorer.network.browser.removeHostConfirmButton` · high
- review (an approve/deny gate, not a read-through) → `beoordelen` / `beoordeling` · macOS renders "Review
  Changes…"→"Bekijk wijzigingen…", but that is the look-over sense; the rename-review modal is a per-row allow/deny
  decision, so the stronger `beoordelen` fits · tentative
- rename cycle → `cyclus van naamwijzigingen`; "while rotating these files" → `terwijl deze bestanden van naam wisselen`
  · "roteren" reads mechanical in Dutch for files swapping names; the badge stays the compact `(cyclus)` · tentative
- convert (file contents) → `converteren` · macOS ("Converting…"→"Converteren…", AppKit "bij het converteren van …") ·
  high
- temporary → `tijdelijk` · macOS ("temporarily unavailable"→"tijdelijk niet beschikbaar") · high
- image search (the feature, as referenced from outside Settings) → `het doorzoeken van afbeeldingen` · the Settings
  card is `settings.mediaIndex.card` "Afbeeldingen doorzoeken", so prose references reuse that verb phrase rather than
  coining "zoeken in afbeeldingen" · high
- "Indexing images" (in-progress status label) → `Afbeeldingen worden geïndexeerd` · the passive-progress form the
  glossary already uses ("wordt gedownload", "wordt geïnstalleerd"); a bare `Afbeeldingen indexeren` would read as the
  infinitive "to index images" and collide with the sibling Settings labels · high
- Recent-past events in status/tooltip prose take the PERFECT, not the simple past · macOS nl ("is mogelijk verplaatst
  of verwijderd"); `driveIndex.tooltipCoalesced*` now reads "macOS is … het spoor … kwijtgeraakt" (was "macOS raakte …
  kwijt") · high
- No `sameAsSourceJustification` needed: all 54 values differ from English.

REVIEW FLAGS (bulk-rename / image-index quality pass):

- **`hernoemen` still survives OUTSIDE these 54 keys** and contradicts the settled `naam wijzigen` ruling:
  `queue.row.label` rename arm ("Bezig met hernoemen"), `onboarding.stepAi.table.rowRename`/`.renameWithout`/
  `.renameWith` ("Massaal hernoemen", "hernoempatroon"), and four `errors.*` prose lines ("verplaatst, hernoemd of
  verwijderd"). Worth one locale-wide sweep; the past participle in flowing prose is the most defensible of them.
- `askCmdr.renameReview.rename` "Rename {count} files" → `Wijzig # bestandsnaam` / `Wijzig # bestandsnamen` — the
  compact "change N file names" shape, chosen over the literal "Wijzig de namen van # bestanden" so the primary button
  stays short. Subjective.
- `askCmdr.renameReview.title` "Review file renames" → `Naamwijzigingen beoordelen` (drops the explicit "file", which
  the modal context supplies). Subjective.

From the image-index-indicators pass (11 `fileExplorer.imageIndex.*` + 2 `settings.mediaIndex.showFileStatusIcons.*`;
mined `_ignored/i18n/nl/`, 2026-07-22):

- badge / status badge (small overlay marker on a file/folder icon showing image-index state) → `markering` /
  `statusmarkering` (plural `markeringen`/`statusmarkeringen`) · macOS Finder `Markeer` (Mark/Flag verb, key QK4) +
  in-catalog `gemarkeerd` (the sibling `settings.mediaIndex.alwaysIndex*.description` "die de gebruiker heeft
  gemarkeerd"). Chosen over the loanword `badge` (Microsoft NLD/BEL keeps "badge"→"badge", but only for the
  gamification/reputation sense — ambiguous here), and over Thunar's Tier-3 `embleem`. `markering` is native,
  unambiguous for a small visual status marker, and screen-reader-clean · high
- image (in these file-list/status labels) → `afbeelding` / `afbeeldingen` · the technical/label register per the
  settled network-image split (image→afbeelding for labels, foto→foto''s for warm network-drive rows). "image
  file"→`afbeeldingsbestand` (compound) in the settings label · high
- image search (the feature, referenced from tooltips + aria) → reuses the settled `het doorzoeken van afbeeldingen`
  (Settings card `settings.mediaIndex.card` = "Afbeeldingen doorzoeken"); "Indexed for image search"→"Geïndexeerd voor
  het doorzoeken van afbeeldingen", "Image search is off …"→"Het doorzoeken van afbeeldingen staat uit …" · high
- indexed (status) → `geïndexeerd`; re-indexed → `opnieuw geïndexeerd`; "couldn''t be indexed" →
  `Kon niet worden geïndexeerd` (macOS passive "kon niet worden …", gentle, avoids bare "mislukt" per Cmdr voice) · high
- waiting to be indexed → `Wacht op indexering` · index→indexeren, noun `indexering` (cf. "achtergrondindexering") ·
  high
- still working (progress tail) → `nog bezig` · reuses the running-status `Bezig` family (queue.row.status) · high
- Settings toggle LABEL register → infinitive-final, matching the sibling `settings.mediaIndex.enabled.label` "Inhoud
  van afbeeldingen indexeren": "Show status badges on image files"→"Statusmarkeringen op afbeeldingsbestanden tonen".
  Its DESCRIPTION uses the imperative, matching the sibling `enabled.description` "Lees de tekst …": "Add a small
  badge…"→"Voeg … een kleine markering toe …" · high
- No `sameAsSourceJustification` needed: all 13 values differ from English.

REVIEW FLAGS (image-index-indicators pass):

- **badge → `markering`/`statusmarkering`**: no Tier-1 Apple term for an icon-overlay status marker exists in the pile
  (macOS has no "insigne" hit; the `BADGE_AX_LABEL` string is the app-icon count badge, a different object). Grounded on
  the native `markering` + in-catalog `gemarkeerd`, but confirm it reads right vs the loanword `badge`.
- `drive.ariaLabel` "Image search status for this drive" → "Status van het doorzoeken van afbeeldingen voor deze schijf"
  keeps the feature-name phrase for consistency; the double `van` is grammatical but slightly heavy. Acceptable for an
  aria-label (clarity over brevity).

From the image-indexing progress/settings pass (12 keys: 3 card titles, the Semantic search card, the "Indexing now"
badge; mined `_ignored/i18n/nl/`, 2026-07-23):

- search by description / search photos by description (the semantic-search feature) → `zoeken op beschrijving` /
  `Foto''s op beschrijving zoeken` (toggle label, infinitive-final, matching the sibling
  `settings.mediaIndex.enabled.label` "Inhoud van afbeeldingen indexeren") · reuses the already-translated
  `settings.mediaIndex.clip.ready` "zoek je foto''s op beschrijving" and `clip.description` phrasing; `foto` per the
  settled network-image split (warm user-facing photo rows use `foto''s`, ICU-doubled) · high
- Apple silicon → kept verbatim · Apple's Dutch macOS keeps "Apple silicon" untranslated (M-series chip family name);
  brand/hardware token · high. `clip.notSupported` = "Zoeken op beschrijving vereist een Mac met Apple silicon."
- reclaim / free (disk space, the delete-model button + confirm) → `vrijmaken` · matches the in-catalog
  `settings.mediaIndex.reclaim.*` ("vrij te maken", "vrijgemaakt"); "reclaim {size}"→"{size} vrijmaken", "This frees
  {size}"→"Dit maakt {size} vrij" · high
- Enable indexing (card title) → `Indexeren inschakelen` · glossary enable→inschakelen + index→indexeren · high
- Folders to index (card title) → `Mappen om te indexeren` · glossary folder→map + index→indexeren; friendlier "om te +
  infinitief" over the stiffer "Te indexeren mappen" for a card heading · high
- Indexing now (badge tooltip + progress heading, both source hash 44501db) → `Wordt nu geïndexeerd` · passive-progress
  form (cf. "wordt gedownload"/"wordt geïnstalleerd"), contrasts cleanly with the sibling badge `pending` "Wacht op
  indexering"; used identically for the file badge and the progress-summary heading · high
- semantic search model (delete-confirm title) → `het model voor semantisch zoeken` · reuses the settled `clip.title`
  "Semantisch zoeken"; the "model voor X" construction reads more naturally than a "semantische-zoekmodel" compound ·
  high
- keyword search / tag search (delete-confirm reassurance) → `zoeken op trefwoord` / `zoeken op tag` · keyword→trefwoord
  (standard NL, MS), tag→tag (in-catalog `settings.listing.showTags` "Tags tonen") · high
- "couldn''t be removed just now" (delete-model failure, gentle) → "kon nu even niet worden verwijderd" · the "nu even
  niet" idiom carries "just now / not at this moment" better than the past-tense "zojuist"; "Try again in a moment"→
  "Probeer het zo opnieuw"; avoids bare "mislukt"/"fout" per Cmdr voice · high
- No `sameAsSourceJustification` needed: all 12 values differ from English.

From the dialog-polish pass (`fileOperations.json`, 2026-07-23): the delete dialog swapped its Prullenmand/Verwijderen
picker for a "Move to trash" switch plus a matching confirm button, and the copy/move/compress dialog groups the source
path and the destination volume+path under "From" and "To" headings.

- "Move to trash" (`delete.trashSwitch`; switch in the delete dialog, on = prullenmand, off = permanent delete) → Naar
  prullenmand · identical to this file's `transferDialog.titleVerbOnly` `other {Naar prullenmand}` arm, so the switch
  and the confirm button read as one pair. macOS Finder's fuller "Verplaats naar prullenmand" (AL13/N153) stays the
  SENTENCE form, matching the gerund arm "Naar prullenmand verplaatsen"; a switch takes the terse label · high
- "Delete" (`delete.confirmDelete`; destructive confirm button while the switch is off) → Verwijder · settled imperative
  button form, identical to `transferDialog.titleVerbOnly`'s `delete {Verwijder}` arm · high
- "From" / "To" (`transferDialog.sourceGroupTitle` / `targetGroupTitle`; headings over the source path and over the
  destination volume + path) → Van / Naar · Total Commander nl (`662="Van: "`, `663="Naar: "`) and Double Commander nl
  ("Van:"/"Naar:") both ship this label pair in the same copy/move dialog; macOS "Verplaats naar" confirms "naar" for a
  destination. The settled nouns bron / bestemming stay for the destination CONTROLS; the headings take the light
  prepositional pair the English uses. "Naar" doubles as the trash-switch preposition, but the two live in different
  dialogs, so there's no in-screen clash · high
