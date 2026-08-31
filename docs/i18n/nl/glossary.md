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
  "Disk Utility", "First Aid", "Activity Monitor", "Spotlight", "Terminal", "Finder" stay English (do-not-translate) ·
  high.
- **"Get Info" and "Locked" are NOT do-not-translate in `nl`** — Apple localizes both, so term-choice principle 1 says
  we follow the Dutch Finder: `Get Info` → `Toon info`, the Info-panel checkbox `Locked` → `Beveiligd`. See § Get Info
  en Beveiligd below for the evidence · high.

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
- view mode: Brief / Full → **Beknopte weergave** / **Volledige weergave** · compositional ("Weergave" from glossary +
  beknopt/volledig), and it's the name in the View menu (`menu.view.briefView`) · high. ❌ NOT `Kort`: Settings used to
  say `de weergave Kort`, which named a view the menu bar doesn't have. Every key says `Beknopte weergave` now.
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
- send → **Verstuur** (button) / **X versturen** (dialog title) / versturen (verb) · macOS Finder ("Send"→"Verstuur") ·
  high. ❌ NOT `Stuur`: an earlier pass picked it for being shorter and forked the Send family three ways. The
  catalog-wide form is macOS's `Verstuur` (see the top-of-file glossary entry), and the button-vs-title split is the one
  legitimate variation. See § Eén ding, één naam.
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
- queue (noun) → wachtrij · Double Commander + Total Commander + Thunar all use "wachtrij" (no macOS term); Microsoft
  terminology confirms queue→wachtrij (NLD/BEL) · high. ⚠️ The compound for the window is now **Bewerkingenwachtrij**,
  NOT "Overdrachtswachtrij" — see the operation-queue rename pass (2026-08-08) at the end of this file.
- transfer (a copy or move of data, noun) → overdracht (plural overdrachten) · standard NL ("overdracht" for a data
  transfer) · high. Scope: the copy/move sense ONLY (`transferProgress.pauseAria` "Pause this transfer" → "Pauzeer deze
  overdracht", `stallUnknown`, `smbNativeNote`). It is NOT the word for the queue window or for a queue row: those name
  the wider category and take `bewerking` (operation-queue rename pass, 2026-08-08).
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
- `fileOperations.transferProgress.queuedToast` + `.queuedToastCount`: the EN puts the count phrase ("1 operation")
  leading; Dutch needs the verb to agree with the count, so the count FRAGMENT carries the finite verb ("gaat #
  bewerking" / "gaan # bewerkingen") and the host sentence wraps it as "Er {countText} deze voor, dus deze wacht op zijn
  beurt." Renders "Er gaat 1 bewerking deze voor" / "Er gaan 3 bewerkingen deze voor". Token kept in the same slot. ⚠️
  The two keys are ONE unit: never re-translate either half alone, or the verb stops agreeing (noun updated in the
  operation-queue rename pass, 2026-08-08).

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
  de-word "map" (cf. in-file "Vind hem in de bewerkingenwachtrij"); operation verb "het kopiëren"/"het verplaatsen"
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

For the image-search index status badges (11 `fileExplorer.imageIndex.*` + 2
`settings.mediaIndex.showFileStatusIcons.*`; mined `_ignored/i18n/nl/`, 2026-07-22):

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

REVIEW FLAGS (image-search index status badges):

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

From the drive-indexing master-switch review (5 keys: `fileExplorer.navigation.driveIndex.refusedIndexingOff` /
`.tooltipIndexingOff` / `.menuIndexingOffNote`, `settings.indexing.masterOffNote` / `.overriddenBadge`; re-mined
`_ignored/i18n/nl/`, 2026-07-27):

- **"drive indexing" as a SUBJECT or object in running prose → `het indexeren van schijven`**, NOT the bare toggle label
  `Schijf indexeren` · the catalog's own settled shape for a feature named inside a sentence:
  `fileExplorer.imageIndex.drive.off` = "Het doorzoeken van afbeeldingen staat uit voor deze schijf.",
  `indexing.enrich.queued` / `settings.mediaIndex.importanceThreshold.waitingForImportance` = "Het indexeren van
  afbeeldingen begint …", `search.imageResults.networkOff` = "Schakel het indexeren van afbeeldingen … in via
  Instellingen". KDE Dolphin nl corroborates the construction ("File Indexing"→"Indexeren van bestanden") · high. A bare
  `Schijf indexeren staat uit …` garden-paths as an imperative ("index the drive!") and drops the article Dutch wants on
  a nominalized infinitive. The plural `schijven` also carries the global reading the master-switch strings need, which
  the singular label does not.
- **The navigation path still quotes the label verbatim**: `Zet het aan bij Indexeren > Schijf indexeren.`
  (`settings.section.indexing` = "Indexeren", `settings.indexing.enabled.label` / `settings.section.driveIndexing` =
  "Schijf indexeren"). Separator `>` mirrors EN per key · high
- **`het indexeren …` also fixes pronoun agreement**: the nominalized infinitive is a het-word, so the follow-up "Zet
  het aan" now has a real antecedent (with `Schijf indexeren` as subject, "het" pointed at nothing) · high
- **Register split for this concept** (same split the image family already uses): user-facing prose takes the verb
  phrase `het indexeren van X`; terse slots (badges, aria labels, internal descriptions) take the compound noun
  `schijfindexering` / `beeldindexering` / `achtergrondindexering`. `indexing.status.ariaLabel` = "Status van
  schijfindexering" is the in-catalog precedent; Microsoft terminology confirms the compound pattern ("content
  indexing"→"inhoudsindexering") · high
- "Off with drive indexing" (`overriddenBadge`, a badge on a row the master switch overrode) →
  `Uit met schijfindexering` · comitative `met`, mirroring EN's shape and length (24 chars vs 23); the compound noun
  keeps it badge-short where "Uit met het indexeren van schijven" would not fit · high
- "stays unindexed" → `wordt niet geïndexeerd` · plainer than the coinage "blijft ongeïndexeerd" and matches the
  in-catalog negation style ("Kon niet worden geïndexeerd", "nog niet geïndexeerd") · high
- "keeps its own on or off choice" → `onthoudt of hij aan of uit staat` · `schijf` is a de-word, so the pronoun is
  `hij`; replaces the coined compound "aan- of uitkeuze", which does not form in Dutch · high
- "picks up where it left off" → `pakt … de draad weer op` · the standard Dutch idiom · high
- No `sameAsSourceJustification` needed: all five values differ from English.

REVIEW FLAGS (drive-indexing master-switch review):

- `settings.indexing.overriddenBadge` "Uit met schijfindexering": Dutch `met` can read instrumentally ("off BY MEANS OF
  drive indexing") as well as comitatively ("off ALONG WITH drive indexing"), the sense intended. In the badge's slot
  (beside a visibly dimmed row) the comitative wins, but a native reviewer should confirm. Unambiguous alternatives are
  longer or lose the "off": "Mee uit met schijfindexering" (28 chars), "Volgt schijfindexering".
- **Pre-existing string changed for consistency**: `fileExplorer.navigation.driveIndex.tooltipDisabled` "Indexeren staat
  uit voor deze schijf." → "Het indexeren staat uit voor deze schijf." One word, to match the article the sibling
  `imageIndex.drive.off` already carries and to keep the two tooltips that alternate on the SAME dot (per-drive off vs
  master off) grammatically parallel.
- **Not changed, flagged**: `onboarding.stepOptional.indexing.descIntro` = "Schijf indexeren is supergaaf!" is the one
  remaining place the bare toggle label acts as a sentence subject. It sits directly under the identical heading, so the
  echo is deliberate there, but a locale-wide ruling would move it to "Het indexeren van schijven is supergaaf!".

## Schijfindex: de wijzigingscontrole (2026-07-28)

- **"Checking for changes" (run-kind header) → `Controleren op wijzigingen`** · Nautilus NL models the exact
  construction ("Unable to poll “%s” for media changes" → "Kan ‘%s’ niet **controleren op** mediawijzigingen"), and
  `wijzigingen` is catalog-settled (`Recente wijzigingen inhalen`) · high.
- **"Update the file list" → `Bestandenlijst bijwerken`** · composed from the settled siblings `Bestandenlijst opslaan`
  - `Index bijwerken` · high.
- **"the check running right now" → `de controle die nu bezig is`** · reuses `controle` as this catalog's settled word
  for a full check (`tooltipCoalesced`: "de volgende volledige controle van Cmdr") and that string's closing
  `zet dat weer recht` · high.

## Vastgelopen overdracht: het stall-bericht (2026-07-31)

Eight keys for the stalled-transfer notice (`fileOperations.transferProgress.close`/`.stallNotice`/
`.stallWaitingDestination`/`.stallWaitingSource`/`.stallUnknown`/`.stallInFlight`/`.stallLogHint`, `queue.row.stalled`);
mined `_ignored/i18n/nl/`, 2026-07-31.

- progress (of a transfer) → `voortgang` · macOS AppKit/Finder ("Progress"→"Voortgang", "Toon kopieervoortgang"),
  Microsoft terminology ("Progress"→"Voortgang", NLD/BEL), Thunar ("File Operation Progress"→"Voortgang van
  bestandbewerking"), Double Commander ("Show operations progress"→"Toon voortgang van bewerkingen); already in-catalog
  as `Voortgang grootte` / `Voortgang bestanden` · high
- "No progress for {duration}" → `Al {duration} geen voortgang` · the "al X geen Y" construction is the natural Dutch
  for an elapsed-since-anything-happened line; the literal `Geen voortgang gedurende …` reads bureaucratic. Used for
  BOTH the dialog line (with a period) and the queue row (without), matching the EN pair · high on the term, `tentative`
  on the construction
- respond (a device/share answering) → `reageren` · macOS AppKit ("… did not respond to the request for services"→"…
  reageert niet op het verzoek om voorzieningen"). Microsoft's `beantwoorden` is the reply-to-a-message sense, not this
  one; macOS is Tier 1 · high
- "Waiting for X to respond." → `Wachten tot X reageert.` · macOS Finder's own two waiting shapes are
  `Wachten op <noun>` ("Waiting for disc drive…"→"Wachten op schijfeenheid…") and `Wachten tot <clause>` ("Waiting for
  transfer with '^0' to complete…"→"Wachten tot overdracht met '^0' is voltooid…"); the clause form fits a verb like
  `reageert` · high
- source (the side being read FROM) → `bron` · Double Commander ("Source"→"Bron", "Waiting for access to file
  source"→"Wachtend op toegang tot bestandsbron"), Microsoft terminology ("source"→"bron", NLD/BEL) · high
- destination (the side being written TO) → `bestemming` · glossary above (macOS "at Destination"→"op bestemming");
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
- close (dialog button that leaves the work running) → `Sluit` · glossary above; the catalog-wide rendering of "Close"
  (10 keys incl. the sibling `fileOperations.errorDialog.close`), and clearly distinct from `Annuleer` next to it · high

Notes:

- `stallInFlight` splits only the noun + copula across the plural branches
  (`{count, plural, one {# bestand is} other {# bestanden zijn}} nog open en misschien al gedeeltelijk geschreven.`), so
  the shared tail carries both predicates on one copula. Same technique as `askCmdr.renameUndo.*`'s
  `{count, plural, one {het is} other {ze zijn}}`. Dutch CLDR categories are `one` / `other`.
- `stallUnknown` refers to the transfer as `hem` (de-word `overdracht`), matching `queuedToast` / `backgroundedToast`
  ("Vind hem in de bewerkingenwachtrij"), and reuses `laat … op de achtergrond doorlopen` verbatim from `queueTooltip`.
  The comma before `of` follows both the EN source and the in-catalog habit ("Probeer opnieuw, of verbreek de
  verbinding").
- Voice rule held: no `fout` / `mislukt` anywhere in the eight values.
- No `sameAsSourceJustification` needed: all eight values differ from English.

REVIEW FLAGS (stalled-transfer pass):

- `Al {duration} geen voortgang` — grammatical and idiomatic, but no pile string phrases an elapsed-stall this way
  (nothing in the pile has the concept). Alternatives, if a native reviewer prefers them:
  `Geen voortgang in {duration}`, `{duration} geen voortgang`.
- `De overdracht komt niet meer vooruit.` — see the term row; `ligt stil` is the more idiomatic standstill phrase but
  risks reading as "paused" in a dialog that has a real paused state.

## Gekopieerd pad: de klembordbevestiging (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

Eén sleutel: de regel van de informatiemelding na ⌘⌥C. Het pad staat eronder op een eigen regel in een
vaste-breedtelettertype, dus het is GEEN plaatshouder in de zin: de zin eindigt op een dubbele punt en moet zonder het
pad kloppen.

- **"Copied the path, it's now on your clipboard:" → `Pad gekopieerd, het staat nu op het klembord:`** · hergebruikt
  `clipboard → klembord` en `path → pad` uit het glossarium (macOS AppKit) · high. Object-dan-voltooid-deelwoord volgt
  de zustermeldingen (`{countText} onderdelen gekopieerd`), en `op het klembord` matcht `clipboard.empty` ("Geen
  bestanden op het klembord"). Geen bezittelijk voornaamwoord: er is er maar één.
- Geen `sameAsSourceJustification` nodig: de waarde wijkt af van het Engels.

## Operation queue: de hernoeming van Overdrachtswachtrij (2026-08-08)

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
- No `sameAsSourceJustification` needed: all 14 values differ from English.

REVIEW FLAGS (operation-queue rename pass):

- **`Bewerkingenwachtrij` vs `Bewerkingswachtrij`** — the linking morpheme is the one open call. `-en-` was chosen for
  the pair with `Bewerkingenlogboek` and the plural-content reading; Microsoft's `-ing`-noun compounds would give `-s-`.
  Whichever a native reviewer prefers, the queue and the log must move together.
- **`bewerking` also reads as "edit"** (the archive_edit queue row is literally "Bezig met archief bewerken"). macOS
  Tier 1 uses `bewerking` for a file operation anyway, and the log window has shipped that reading since 2026-07-10, so
  the risk is accepted rather than routed around.
- **`queue.empty.body` was left alone** (its English didn't change): it still says "Kopieer-, verplaats- en
  verwijderacties verschijnen hier …", which names the three transfer kinds and `acties`, not `bewerkingen`. Consistent
  with its own English, but if the English empty state is ever widened, this is the key that has to follow.
- **Keys added later to `queue.json` must reuse `Bewerkingenwachtrij` / `bewerking`.** English already has
  `queue.failureToast.action` "Show in operation queue", `queue.failureToast.summary`, `queue.chip.failed`, and
  `queue.chip.ariaLabel` ("Open the operation queue"), which `nl` doesn't carry yet. Done in the corner-chip +
  failure-notice pass below.

## De voortgangschip en het niet-voltooid-bericht (2026-08-08)

Nine new `queue.json` keys for two surfaces: the main window's ~80 px corner progress chip (`queue.chip.*`) and the
failure notice plus its dismissable queue rows (`queue.failureToast.*`, `queue.row.dismiss*`,
`queue.toolbar.dismissAll`). The window name and the head noun come from the operation-queue rename pass above; this
section records only what was new.

- **dismiss (remove a finished-with-a-problem row from a list; nothing is undone, retried, or deleted) → `Wis` (button)
  / `wissen`** · macOS Finder + AppKit Tier 1 render "Clear Menu" → "Wis menu" (key `A29`), which clears a recent-items
  LIST without deleting anything: the same act. Already the in-catalog rendering of "clear"
  (`settings.fileSystemWatching.clearIndex` "Index wissen", `settings.sidebar.clearSearch` "Zoekopdracht wissen",
  `shortcuts.section.pressEscToClear` "Druk op ESC om te wissen"); the imperative stem `Wis` follows the button rule ·
  high. ⚠️ This is a SECOND sense of "dismiss" in this catalog, deliberately split from the settled `dismiss → Sluit` /
  `Sluit melding` (a toast or popover is CLOSED; a list row is CLEARED, and "Sluit deze bewerking" would read as "close
  this operation", which is not what the button does).
  - ❌ NOT `Verwijder uit lijst` (the settled `remove from list`, `goToPath.dialog.removeFromList`): honest, but it
    leads with the delete verb on a row whose own label can be "Bezig met verwijderen", and it is far too long for a
    `size="mini"` row button. ❌ NOT `Verberg`: implies the row can come back, and it can't.
  - "Dismiss all" → `Wis alles`, the `<imperatief> alles` shape the toolbar already uses (`Pauzeer alles`,
    `Hervat alles`, macOS "Selecteer alles") · high.
  - The row aria takes the family shape `<Werkwoord> deze bewerking`
    (`Pauzeer/Hervat/Annuleer/Selecteer deze bewerking`), so `Wis deze bewerking` · high.
- **"Couldn''t finish X" (failure-notice headline) → `X niet voltooid`** · built on the row's own status word
  `Niet voltooid` (`queue.row.status` failed arm) so the toast, the row, and the chip say one thing. Each arm is the
  matching `queue.row.label` verb with "Bezig met " stripped, in the nominalized infinitive Dutch headlines take:
  `Kopiëren niet voltooid`, `Verplaatsen niet voltooid`, `Verwijderen niet voltooid`,
  `Naar prullenmand verplaatsen niet voltooid`, `Hernoemen niet voltooid`, `Map aanmaken niet voltooid`,
  `Bestand aanmaken niet voltooid`, `Archief bewerken niet voltooid`, `other` = the bare `Niet voltooid` · high. No
  `fout` / `mislukt` anywhere, per the voice rule.
- **"N operations couldn''t finish" (count headline) → `{countText} bewerking is` / `{countText} bewerkingen zijn` + the
  shared tail ` niet voltooid`** · the copula lives INSIDE the plural branches and the predicate is shared, per
  `style.md` § Plurals · high. Renders "1 bewerking is niet voltooid" / "3 bewerkingen zijn niet voltooid".
- **"Show in operation queue" (notice button) → `Toon in de bewerkingenwachtrij`** · glossary `show → Toon` (macOS
  Finder) + the window name, lowercased mid-sentence with its article exactly as the catalog already writes it
  (`transferProgress.queueTooltip` "… in de bewerkingenwachtrij (F2)", `queuedToast` "Vind hem in de
  bewerkingenwachtrij") · high.
- **"Open the operation queue (to see why)" (the chip's promise) → `Open de bewerkingenwachtrij`
  (`… om te zien waarom`)** · glossary `open → Open` (macOS Finder imperative) + the same article-and-lowercase
  treatment · high.
- **"percent", spelled out for screen readers → `procent`** · the Dutch word, which VoiceOver nl reads naturally; the
  `%` SIGN stays in the visual tooltip and takes NO space before it in Dutch (`{percentText}%`, matching the catalog's
  "Zoom naar 100%" and `lowDiskSpace` "{percentText}%") · high.
- **"items" in the chip tooltip → `onderdeel` / `onderdelen`** · the settled macOS Finder term (glossary above); covers
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

No `sameAsSourceJustification` needed: all nine values differ from English.

REVIEW FLAGS (corner-chip + failure-notice pass):

- **`Wis` for "dismiss"**: `wissen` also renders "erase" in Dutch macOS ("Wis schijf" = Erase Disk), so on a row in a
  FILE manager it carries a faint destructive echo, and the queue window has a real destructive button next to it
  (`Terugdraaien`). Chosen anyway because Tier-1 "Wis menu" is the exact list-clearing analog, it is what this catalog
  already says for "clear", and it is the only candidate short enough for a mini button with the `x` glyph beside it.
  Alternatives if a native reviewer disagrees: `Verwijder uit lijst` (honest, long, delete-flavoured) or `Verberg`
  (safe, but implies the row comes back).
- **`Hernoemen niet voltooid`** follows `queue.row.label`'s rename arm ("Bezig met hernoemen") so the toast, the chip,
  and the row agree today. It therefore ADDS one more instance of `hernoemen`, which the 2026-07-20 quality pass ruled
  against in favour of the settled `naam wijzigen` (macOS + Microsoft, high) and flagged for a locale-wide sweep. When
  that sweep runs it must move `queue.row.label` and this arm TOGETHER (to `Naam wijzigen niet voltooid`), never one
  alone.
- **Overflow risk, `queue.failureToast.title` trash arm**: `Naar prullenmand verplaatsen niet voltooid` is 42 characters
  against English's 31, in a ~360 px toast. It should fit on one line, but it's the longest string on either new surface
  and the first place to look if the toast title wraps.
- **`Toon in de bewerkingenwachtrij`** is 30 characters against English's 23, on the notice's action button. The article
  can't be dropped in Dutch; the fallback if it clips is `Toon in de wachtrij`, which loses the window name the `@key`
  description asks us to keep.
- **`Wis alles`** can read as "clear the whole list" rather than "clear the rows that couldn't finish", which is the
  same ambiguity English's "Dismiss all" carries. Kept deliberately.

## Het losse conflictvenster (`fileOperations.operationConflict.*`, 2026-08-09)

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
- No `sameAsSourceJustification` needed: both values differ from English.

REVIEW FLAG: `Bezig in {destination}` is the terse fallback arm and reads slightly clipped on its own; the fuller
`Bezig in de map {destination}` is wrong whenever the operand isn't a folder, so the short form stands.

## De knop voor een lege wachtrij: "Background" (2026-08-09)

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
- No ICU apostrophes in either value; no placeholders. No `sameAsSourceJustification` needed (both differ from English).

REVIEW FLAGS (empty-queue button pass):

- **Width**: `Op de achtergrond` is 17 characters against English's 10, on the same button that shows `Wachtrij` (8) in
  its other state, so the dialog's primary button changes width noticeably between the two states. If it crowds the
  neighbouring `Pauzeer` / `Annuleer`, the fallback is the TC-nl bare `Achtergrond` (11), which costs the action reading
  and the exact-containment aria.

## Het stoppoortje: afsluiten terwijl er nog werk loopt (`main.quit.*`, 2026-08-10)

Seven `main.json` keys for the modal Cmdr raises when the user quits (⌘Q, the menu, or closing the main window) while a
copy, move, delete, trash, or archive edit is still running: a question title, a reassuring body, a list heading, a live
countdown plus its screen-reader name, and the two buttons. The head noun `bewerking` and the queue's own verb family
come from the operation-queue rename pass above; this section records what was new.

- **"Quit" (the app stopping, in a sentence) → `stoppen`; the imperative button → `Stop`** · macOS Dutch Tier 1 uses
  `stoppen`, NOT `afsluiten`: Finder's `A17` "The Finder can't quit because some operations are still in progress." →
  "De Finder kan niet worden gestopt, omdat er nog bewerkingen worden uitgevoerd.", `A19` (singular) → "… omdat er nog
  een bewerking wordt uitgevoerd …", AppKit "Quit Anyway" → "Stop toch", the menu "Stop Finder". Already the glossary's
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
    `Log uit`, and the catalog already ships `Herstart` (glossary above) and `inloggen`/`Log in`. Microsoft's
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
    which is the opposite outcome. ❌ NOT `Later` (the settled dismiss-for-now word, `updates.later`): the countdown is
    deleted, not deferred. ❌ NOT `Behoud` (macOS's "Keep"), which is the keep-this-file sense.
  - `Stop niet` (macOS Finder `BN63` "Don't Stop" → "Stop niet") is the attested negative twin and would be defensible,
    but English deliberately frames this positively, and `Werk door` reads as the friendlier of the two.
- **"Quit now" (primary, destructive) → `Stop nu`** · `Stop` (above) plus `nu`, which does the same load-bearing work as
  English's "now": the app quits either way when the countdown ends, and this skips the wait · high.
- **"Still running" (heading over the operation rows) → `Nog bezig`** · `Bezig` is the queue's own running-state word
  (`queue.row.status` running arm, `queue.row.label` fallback), so the heading and the rows under it speak one
  vocabulary; `nog` carries "still" · high.
- No `fout` / `mislukt` anywhere in the seven values, per the voice rule. No apostrophes, so no ICU doubling was needed.
  No `sameAsSourceJustification`: all seven differ from English.

REVIEW FLAGS (quit-gate pass):

- **`Werk door`** is the one coined value here. It is unambiguous against "later" and against "cancel the operations",
  but no source ships this button. Alternatives a native reviewer might prefer: `Blijf werken` (closer to the English
  wording, 12 chars), `Ga door met werken` (explicit, 18), or the attested-but-negative `Stop niet`.
- **`opruimen` for "clears away"** is judgment, not evidence: the pile has no string for an app tidying up its own
  half-written output. `verwijdert` is the obvious literal and was rejected on the delete-collision above.
- **Title length**: `Stoppen terwijl er nog 3 bewerkingen worden uitgevoerd?` is 55 characters against English's 45. It
  follows macOS's own phrasing, so shortening it costs the Tier-1 match; if the dialog title wraps, the terser
  `Stoppen terwijl er nog 3 bewerkingen lopen?` (43, using `queue.empty.body`'s own "terwijl ze lopen") is the fallback.
- **Body length**: 174 characters against English's 138, the longest of the seven. It is a wrapping body paragraph, so
  this should be fine, but it is the first place to look if the dialog grows taller than expected.

## Usage stats: "anonieme" dropped, "een willekeurige id" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, 2026-08-12)

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
- No `sameAsSourceJustification` needed: every value differs from English.

## De terugdraaibevestiging en de rij die op antwoord wacht (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`, 2026-08-13)

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
  bewerking die blijft lopen (`queueTooltip`, `backgroundedToast`), en `hoofdvenster` staat al in deze woordenlijst ·
  high.
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
- Geen `sameAsSourceJustification` nodig: alle acht waarden verschillen van het Engels.

REVIEW FLAG: `Antwoord nodig` is gemunt, niet gevonden in de pile; als een moedertaalspreker meekijkt, is dit de eerste
regel om te toetsen (alternatief `Jouw antwoord nodig`, dat wel "your" meeneemt maar breder is).

## De keten-hernoemtoast die meetelt (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, 2026-08-18)

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
- Geen ICU-apostrof nodig: de waarde bevat geen enkele rechte `'`, en de aanhalingstekens zijn de gekrulde `‘…’` van de
  locale-brede afspraak.

## De onbevestigde naamwijziging en de onbruikbare naam (`fileExplorer.rename.unconfirmed`/`unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`, 2026-08-18)

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
- Geen ICU-apostrof nodig: geen enkele rechte `'` in de drie waarden, en de aanhalingstekens zijn de gekrulde `‘…’`.

## Voorgestelde bewerkingen: het venster met wat Ask Cmdr voorstelt (`suggestedOps.*`, `commands.suggestedOpsShow.*`, 2026-08-19)

- ops (de door de agent voorgestelde bestandsbewerkingen) → `bewerkingen`; titel `Voorgestelde bewerkingen` · sluit aan
  op "File operations" → "Bestandsbewerkingen" · high
- approve → `Goedkeuren` · ms/standaard NL; gekozen boven het `Accepteer` van macOS, omdat de telvariant ("3 bestanden
  goedkeuren") toestemming geeft in plaats van iets aan te nemen · high
- reject → `Weigeren` · macOS Finder, het paar Accepteer/Weiger in het AirDrop-venster (Tier 1); infinitief, omdat de
  knoppen hier infinitief zijn · high
- "This can't be undone" → `Dit kun je niet ongedaan maken` · macOS Finder, woord voor woord (waarschuwing bij direct
  verwijderen) · high
- suggestion → `suggestie` · al in de catalogus · high

## Dupliceer: de opdracht die in dezelfde map kopieert (`commands.fileDuplicate.*`, 2026-08-19)

- **duplicate (opdracht die de selectie in haar eigen map kopieert) → `Dupliceer`** · macOS Finder `nl`, menu 'Archief >
  Dupliceer' (`N154`), plus 'Dupliceer onderdelen' en 'Dupliceert onderdelen op huidige locatie' (gecontroleerd op macOS
  26.6.1, `Finder.app/Contents/Resources/nl.lproj`, 2026-08-19) · high. Gebiedende wijs, net als de zusters `Kopieer`
  (F5) en `Verplaats` (F6).
- **'Make a copy of the selected files in the same folder' →
  `Maak een kopie van de geselecteerde bestanden in dezelfde map`** · gebiedende wijs, zoals de naburige beschrijvingen
  ('Kopieer geselecteerde bestanden…'); 'dezelfde map' is de map waar de bestanden al staan · high.

## Native menu's: menubalk, contextmenu's, venstertitels (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, 2026-08-19)

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
- **busy (volume in gebruik) → `(bezet)`** · Microsoft-terminologie · high.
- **disconnect → `Verbreek verbinding`** · macOS Finder `nl` („Verbreek”), aangevuld tot een begrijpelijk label · high.
- **Bewust gelijk aan het Engels** (met `sameAsSourceJustification`): `menu.bar.help`, `menu.app.onboarding`,
  `menu.file.open`, `menu.view.zoom`, `menu.zoom.in`, `menu.zoom.percent*`, `menu.view.askCmdr`.

## De terugvalmelding voor de systeem-SMB-verbinding (`fileExplorer.network.osMountFallback.*`, 2026-08-21)

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
  gevestigde `dismiss → Sluit` / MS `dismiss → sluiten` · high. Niet het tweede `dismiss`-gebruik (`Wis`), want hier
  wordt een melding gesloten, geen rij uit een lijst gewist.
- **Cmdrs bezit → `van Cmdr`** (`de directe verbinding van Cmdr`) · de catalogus gebruikt overwegend `van Cmdr`
  (`de AI van Cmdr`, `de index van Cmdr`); de genitief `Cmdrs` komt maar één keer voor · high.

## De weigerberichten van naam wijzigen, nieuwe map en nieuw bestand (`errors.mutation.*`, `errors.volume.*`, 2026-08-23)

Eenendertig sleutels: de regel onder het naamveld (of in een korte melding) wanneer een naamwijziging of een nieuwe
map/bestand wordt geweigerd. RAW-familie, dus enkele apostroffen; `{path}` is een ongecontroleerde invoeging en staat in
de gekrulde `‘…’` van de locale-brede afspraak (het Engels gebruikt rechte `"…"`). Gemijnd in `_ignored/i18n/nl/`,
2026-08-23.

- **locked (de macOS-vlag op een onderdeel) → `beveiligd`; unlock → `de beveiliging opheffen`** · macOS Finder Tier 1:
  het aankruisvak in Toon info heet `Beveiligd` (`AXNODE1`, AppKit `AutosaveButton`), `NE17`/`PE13` zeggen "omdat het
  bestand/onderdeel '^0' beveiligd is", `NE18` "deselecteer 'Beveiligd' en probeer het opnieuw" en `BN43` "de
  beveiliging van dit onderdeel opheffen" · high. ⚠️ Botst met het oudere `errors.write.fileLocked` ("Het bestand is
  vergrendeld"): zie de review-vlag hieronder.
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
  `doelmap` (glossariumrij `destination`), en de catalogus gebruikt `doelmap` al vijf keer tegen `bestemmingsmap` één
  keer · high. Het voornaamwoord bij `map` is `hem` ("Open hem opnieuw"), zoals `Cmdr maakt hem aan`.
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
- Geen `sameAsSourceJustification` nodig: alle 31 waarden wijken af van het Engels.
- Stemregel gehaald: geen `fout` en geen `mislukt` in de 31 waarden.

REVIEW FLAGS (mutation/volume-weigerberichten):

- **`beveiligd` vs `vergrendeld` voor _locked_.** De nieuwe `errors.mutation.fileLocked` gebruikt Apples eigen
  `beveiligd`, omdat de zin de gebruiker naar Toon info stuurt, waar het aankruisvak letterlijk `Beveiligd` heet;
  `vergrendeld` zou hem naar een label laten zoeken dat er niet staat. De oudere `errors.write.fileLocked.title`/
  `.message` en `errors.write.permissionDenied.suggestion.deleteMac` zeggen nog `vergrendeld` (en die laatste laat
  bovendien "Get Info" en "Locked" onvertaald). Een lokale-brede veegbeurt naar `beveiligd` + `Toon info` is de
  aanbeveling; buiten het bestek van deze pass.
- **`De archiefbewerking is niet gestart.`** — `bewerking` leest ook als _edit_ én als _operation_; dat is hier precies
  goed, maar de samenstelling is gemunt en heeft geen bron in de stapel. Alternatief: "Het bewerken van het archief is
  niet gestart."
- **`Verplaats het in plaats daarvan.`** — `Verplaats` is de opdrachtnaam uit het glossarium; `in plaats daarvan` is
  correct maar wat log. Een moedertaalspreker kan `Gebruik daarvoor Verplaats.` prettiger vinden.
- **`naar het andere brengen`** (`renameAcrossArchives`) — `brengen` vermijdt een tweede `verplaatsen` in dezelfde zin;
  bevestig dat het niet te vaag leest.
- **`Er staat niets meer op ‘{path}’.`** — `{path}` kan een bestand of een map zijn, dus de zin noemt bewust geen soort.
  Apples `'^0' bestaat niet meer.` is korter; de locatievorm is gekozen omdat het Engels de plek noemt ("at").

## De twee prullenmandweigeringen (`errors.mutation.trash*`, 2026-08-23)

Twee sleutels die na de pass hierboven zijn toegevoegd, met dezelfde vorm: één platte regel onder het naamveld of in een
korte melding, RAW-familie, enkele apostroffen. Gemijnd in `_ignored/i18n/nl/`, 2026-08-23.

- **"This volume has no Trash" → `Dit volume heeft geen prullenmand`** · `prullenmand` is de gevestigde rij (macOS
  Finder Tier 1) en `volume` de gevestigde rij; `dit volume` sluit aan op de zusterregel `errors.volume.noRoom` ("Er is
  geen ruimte meer op dit volume.") · high. Bewust anders dan het oudere `errors.write.trashNotSupported.message` ("Dit
  volume ondersteunt de prullenmand niet."), want het Engels zegt hier ook `has no`, niet `doesn't support`.
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
- Geen `sameAsSourceJustification` nodig: beide waarden wijken af van het Engels.
- Stemregel gehaald: geen `fout` en geen `mislukt` in de twee waarden.

REVIEW FLAGS (prullenmandweigeringen):

- **`wilde ... niet` vs `weigerde`.** De catalogus gebruikt `weigerde` al voor een afwijzing door macOS zelf
  (`ai.secretError.keychainTitle`: "de macOS-sleutelhanger weigerde toegang"), en de sleutel heet `trashRefused`.
  `macOS weigerde dit naar de prullenmand te verplaatsen.` is dus verdedigbaar, maar klinkt harder dan het Engelse
  `wouldn't`. Bevestig welke van de twee in een melding beter valt.
- **`Verwijder permanent` vs `Definitief verwijderen` in de catalogus.** `menu.file.deletePermanently` en
  `commands.fileDeletePermanently.label` zeggen `Verwijder permanent`, terwijl
  `fileExplorer.functionKeyBar.deletePermanentlyAction` en het glossarium `definitief verwijderen` aanhouden (macOS Tier
  1). Deze nieuwe regel volgt het glossarium. Een locale-brede veegbeurt naar `definitief` is de aanbeveling; buiten het
  bestek van deze pass.

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
- Het Engelse `@key.description` van beide sleutels zegt nog "do NOT translate" over `Get Info` en `Locked`. Dat botst
  met term-keuzeprincipe 1 en met de rest van de catalogus; **REVIEW FLAG** voor de bron-`en`-beschrijving en voor de
  andere acht talen, die waarschijnlijk dezelfde achterblijvers hebben.
- **REVIEW FLAG (`vergrendeld` vs `beveiligd`)**: `errors.write.fileLocked.title/message` en de proza-zin in
  `permissionDenied.suggestion.deleteMac` zeggen `vergrendeld`, terwijl `errors.mutation.fileLocked` en Apple zelf
  `beveiligd` gebruiken voor dezelfde bestandsstaat. Buiten het bereik van deze ronde gelaten; een locale-brede
  gelijktrekking is de moeite waard.

## De negen uitwerp- en verbreekzinnen (`errors.eject.*`)

Elke waarde valt achter een dubbele punt in `fileExplorer.pane.ejectFailedToast`
(`{volumeName} uitwerpen lukte niet: …`) of `fileExplorer.pane.disconnectFailedToast`
(`Verbinding verbreken lukte niet: …`), dus de zin moet als vervolg lezen en mag het wikkelwerkwoord niet herhalen.
`errors.*` is een RAW-familie: gewone apostrofs, geen ICU. Geen van de negen waarden bevat er een.

- **eject → `uitwerpen` / `Werp … uit`** · de al vastgelegde regel (Nautilus, Dolphin, Microsoft; macOS' eigen
  `Verwijder media` botst met _delete_) · high. Voltooid deelwoord `uitgeworpen`; de catalogus had het al in
  `errors.listing.remotePermissionDenied.suggestion` ("werp de share dan uit").
- **drive → `schijf`, een de-woord** · glossaryregel `drive/disk → schijf` · high. Voornaamwoord dus `hij` / `hem`
  ("Werp hem uit", "hij blijft aangesloten"), net als `onthoudt of hij aan of uit staat`.
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
- **REVIEW FLAG (`errors.eject.notAnSmbVolume`)**:
  `Dit is geen netwerkshare, dus er is geen verbinding om te verbreken.` herhaalt `verbreken` na de wikkel
  `Verbinding verbreken lukte niet:`. Het Engels herhaalt daar net zo goed ("Couldn't disconnect: … nothing to
  disconnect"), en Nederlands heeft een object nodig bij `verbreken`, dus de herhaling is bewust. Bevestigen of het in
  de toast stoort.

## De twee knoppen van de prullenmandmelding en de terugzet-familie (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, 2026-08-27)

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

## Aanvullen wat al verstuurd is: het notitievenster bij een automatisch foutrapport (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`, 2026-08-28)

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

## Het selectievenster: bestanden selecteren en deselecteren (`selection.*`, 2026-08-29)

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
- Geen `sameAsSourceJustification` nodig: alle vijftien waarden verschillen van het Engels.

## Eén ding, één naam: de interne driftronde (2026-08-30)

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

## Wat het Engels over zichzelf rechtzette, en wat dat voor `nl` betekende (2026-08-30)

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

## Een half teruggedraaide bewerking afmaken (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`, 2026-08-30)

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

## De terugdraaimelding: wat er weg is, en wat Cmdr met rust liet (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`, 2026-08-31)

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
- Geen `sameAsSourceJustification` nodig: alle achttien waarden verschillen van het Engels.

REVIEW FLAGS: `nadat Cmdr er klaar mee was` ruilt de plaatsbepaling van het Engels in voor genderneutraliteit; dat is de
eerste regel om aan een moedertaalspreker voor te leggen. En let op een divergentie die geen check ziet:
`reason.unverifiable.*` zegt hier `Cmdr kon niet controleren of …`, terwijl de bijna gelijkluidende zus
`askCmdr.renameUndo.skipReason.unverifiable.*` `Cmdr kon niet nagaan of …` zegt. Het Engels verschilt alleen in de
apostrof (`couldn''t` tegen `couldn’t`), dus `desktop-i18n-term-consistency` legt de twee nooit naast elkaar. Eén ronde
die dit paar op `controleren` (het gevestigde woord in deze catalogus) trekt, zou het opruimen.

**Overloop nakijken op de melding.** De redenregels lopen 75–90 tekens tegen 50–60 in het Engels, en
`reason.failed.counted` is de langste van de achttien. Controleer ze tegen de pseudolocale in een smalle melding.
