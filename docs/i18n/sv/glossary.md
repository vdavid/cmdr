# sv glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/sv/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

From the first translation pass (`errors.json`). All sourced from the reference pile or the style guide's settled list.

- **read-only: `skrivskyddad`** · macOS Finder ("skrivskyddad"), MS terminology. The adjective for a read-only
  volume/device. `high`.
- **path: `sökväg`** · macOS Finder, MS ("sökväg"). The filesystem path. `high`.
- **mount (verb/noun): `montera` / `montering`; unmount `avmontera`; remount `montera om`** · macOS, MS, GNOME all use
  "montera"/"avmontera". `high`.
- **permission(s): `behörighet` / `behörigheter`** · macOS Finder ("behörigheter" in Get Info), MS ("behörighet").
  `high`.
- **credentials: `inloggningsuppgifter`** · MS terminology; macOS "uppgifter". The username/password pair. `high`.
- **authentication / authenticate: `autentisering` / `autentisera`** · MS ("autentisering"), macOS. `high`.
- **network: `nätverk`; connection `anslutning`; connect `ansluta`** · macOS ("Anslut till server"), MS. `high`.
- **time out / timed out: `nå tidsgränsen` / `tidsgränsen nåddes`** · MS ("tidsgräns"). Natural Swedish for a connection
  that didn't respond in time. `high`.
- **disk: `disk`; volume `volym`; device `enhet`** · per style guide; "disk" for the physical/logical disk in Disk
  Utility contexts. `high`.
- **Trash (macOS feature, capitalized in copy): `papperskorgen`** · per style guide's trash entry. `high`.
- **retry / try again: `försök igen`** · macOS, MS. The imperative-ish "try again" framing. `high`.
- **navigate here again: `gå hit igen`** · descriptive, no single source; natural Swedish for re-entering a folder.
  `tentative` (composed phrase), low risk.
- **internet connection: `internetanslutning`** · MS, common. `high`.
- **technical details (the expandable section): `tekniska detaljer`** · MS, macOS. `high`.
- **substituted-verb slot (`{verb}`/`{Verb}`/`{gerund}`): frame as `åtgärden {verb}` / `{gerund}`** · the runtime
  substitutes a hardcoded ENGLISH phrase ("copy", "move", "delete", "move to trash" / "copying" etc.;
  `transfer-error-messages.ts`), not a localized one, so wrap it as a foreign noun-phrase the way de/hu/nl do ("åtgärden
  {verb}" = the {verb} action). Titles like `{Verb} failed` → "Det gick inte att slutföra åtgärden {verb}" reads
  awkwardly long, so keep titles tight: "Åtgärden {verb} gick inte". `tentative` (forced by an un-localized slot).

From the `settings.json` pass (2026-06-21). The core file-manager nouns (panel, volym, enhet, mapp, fil, papperskorgen,
server, bokmärke, etc.) live in `style.md` § Terminology; this list is the settings-specific vocabulary.

- **settings: `inställningar`** · macOS SystemSettings ("Inställningar", "Systeminställningar"). `high`.
- **enable / disable: `aktivera` / `stäng av`** · macOS Finder ("Aktivera"), MS (enable → "aktivera", disable → "stänga
  av"). Off-state toggle label "Av". `high`.
- **notification: `avisering`** · MS terminology (notification → "varning / avisering"); macOS SystemSettings
  "Aviseringar". Prefer "avisering"; avoid "notis". `high`.
- **warning: `varning`** · MS terminology, macOS Finder ("Visa varningsmeddelande"). `high`.
- **update (software): `uppdatera` (verb) / `uppdatering` (noun)** · MS ("uppdatera"). "Sök efter uppdateringar".
  `high`.
- **timeout: `tidsgräns`** · MS (time-out → "tidsgräns"). Aligns with errors.json's time-out entry. `high`.
- **cache: noun `cache`, verb `cacha`** · MS ("cache" / "cacheminne"). Keep "cache" for brief UI nouns. `high`.
- **port: `port`** · MS ("port"). Network/MCP port. `high`.
- **column: `kolumn`** · macOS Finder ("Som kolumner", "kolumnvy", "Visa kolumner"). Plural "kolumner". `high`.
- **reset to default: `återställ till förval`; default value prefix `Standard:`** · macOS Finder ("Återställa till
  förval", "Använd som förval"). `high`.
- **item: `objekt`** · macOS Finder/CoreTypes throughout ("Objekt", "markerade objekt"). NOT MS's "artikel". Neuter,
  plural unchanged ("objekt"). `high`.
- **word wrap: `radbrytning`** · standard Swedish IT term; MS's "omslutning" is the wrong sense. `tentative`
  (convention, no direct UI source).
- **branch (git): `gren`** · standard Swedish git term; MS's "förgrena" is the verb sense. `tentative`.
- **repository (git): `git-repository`** · keep the git loanword; MS's "centrallager" is the generic-storage sense.
  `tentative`.
- **startup disk: `startskiva`** · macOS Finder ("Startskiva", "Startskivevärde"). Boot drive. `high`.
- **Privacy & Security (macOS pane): `Integritet och säkerhet`** · macOS SystemSettings. `high`.
- **Full Disk Access (macOS permission): `Fullständig åtkomst till skivan`** · Apple's standard Swedish name; NOT in
  this pile's SystemSettings bundle, taken from Apple convention. `tentative` (flag for native review).
- **Local Network (macOS permission): `Lokalt nätverk`** · Apple's standard Swedish name; same pile-gap caveat.
  `tentative`.

### Cmdr-internal UI names (keep consistent across files)

- **Full view / Brief view (view modes): `Fullständig` / `Kortfattad`** · Cmdr's two file-list view modes. `tentative`
  (Cmdr-coined; review).
- **Name column / Ext column: `Namn` / `Tillägg`** · macOS uses "Namn" and "filnamnstillägg"/"tillägg". `high`.
- **Keyboard shortcuts (section): `Tangentbordsgenvägar`** · standard Swedish (macOS "kortkommandon" is the alt).
  `high`.
- Settings section titles (chosen): Appearance → `Utseende`, Behavior → `Beteende`, File operations → `Filåtgärder`,
  Search → `Sök`, File systems → `Filsystem`, Advanced → `Avancerat`, Developer → `Utvecklare`, Viewer →
  `Förhandsvisning`, Updates & privacy → `Uppdateringar och integritet`, License → `Licens`. `high` (macOS-aligned where
  a term exists).

From the `fileExplorer.json` pass (2026-06-21). The bigger surface (network/SMB browser, MTP, tabs, columns, indexing,
favorites). Reuses the terms above; new ones:

- **host (SMB host in the network browser): `värd`** · MS terminology ("värddator" for host, "värddatornamn" for
  hostname). Short "värd" in tight host-list columns and tooltips; "Värddatornamn" for the explicit Hostname column
  header; "serverlista" for the saved-host list ("Ta bort {värd} från serverlistan?"). `high`.
- **sign in / log in: `logga in`** · macOS Finder ("Logga in"), MS ("logga in"). Same verb for Cmdr''s "Sign in"/"Log
  in" in the SMB flow. Auth-failure phrased calmly ("Det gick inte att logga in"), never a bare "fel". `high`.
- **guest: `gäst`** · MS terminology ("gäst"). Connect as guest = "Anslut som gäst". `high`.
- **share (SMB, network): `delad mapp`** · per style guide''s share entry; plural "delade mappar". The host-list column
  "Shares" (count of shares) is rendered as "Delningar" to stay short; the mounted share itself is a "delad mapp".
  `high`.
- **copy / cut / paste: `kopiera` / `klipp ut` / `klistra in`** · macOS AppKit. `high`.
- **clipboard: `urklipp`** · macOS/Windows Swedish standard ("Urklipp"). "Inga filer i urklipp." `high`.
- **favorites / favorite: `favoriter` / `favorit`** · macOS Finder ("Favoriter"). Section heading + the favorite-row
  noun. `high`.
- **pin / pinned (tab): `nåla fast` / `fäst`** · macOS AppKit ("Nåla fast flik"). Pinned-state label "Fäst"; "Stäng
  ändå?" for the close-pinned confirm. `high`.
- **tab: `flik`** · per style guide. "Ny flik", "Stäng flik", "Gränsen för antal flikar nådd". `high`.
- **refresh / reload (rescan a list): `uppdatera`** · macOS Finder, MS. Network-host refresh and volume-list refresh.
  `high`.
- **index / indexing / scan / rescan: `index` / `indexering` / `genomsökning` / `söka igenom på nytt`** · "indexera"
  (verb), "indexering" (noun); the scan pass is "genomsökning" ("Söker igenom enheten…"), rescan "Sök igenom på nytt".
  macOS shows "Indexerar". `high`.
- **device (phone/camera over MTP): `enhet`** · macOS ("enhet"); same word as drive, context disambiguates. MTP stays
  verbatim. `high`.
- **reachable / unreachable: `nås` / `går inte att nå`** · phrase actively ("Det gick inte att nå {path}", "Fortfarande
  inte nåbar"). `tentative` (composed; low risk).
- **symlink / broken symlink: `symlänk` / `(trasig symlänk)`** · "symlänk" is the standard Swedish for symbolic link;
  macOS uses "symbolisk länk" / "alias". Kept short "symlänk" for the tight placeholder. Used uniformly across
  `fileExplorer.json` + `fileOperations.json` (+ `Symlänksloop` in `errors.json`), no competing form, so promoted from
  tentative. `high`.
- **read-only device/volume: `skrivskyddad enhet` / `skrivskyddad volym`** · from read-only above; agreement per noun
  gender (both en-words → "skrivskyddad"). `high`.

From the `onboarding.json` + `fileOperations.json` pass (2026-06-21). Reuses all terms above; new ones:

- **full disk access (macOS permission): `fullständig åtkomst till skivan`** · lowercase in running copy; matches the
  `Fullständig åtkomst till skivan` SystemSettings pane name (style.md glossary). `high`.
- **grant (a permission): `ge` / `bevilja`** · "ge fullständig åtkomst" for the user action, "beviljad" as a status
  label ("Fullständig åtkomst till skivan beviljad"). macOS uses "bevilja"; "ge" reads warmer in body copy. `high`.
- **revoke (a permission): `återkalla`** · MS terminology ("återkalla"); natural Swedish for turning a granted
  permission off. `tentative` (no direct macOS UI hit, MS-backed).
- **copy / move / delete (transfer verbs): `kopiera` / `flytta` / `radera`; gerunds `kopierar` / `flyttar` / `raderar`**
  · macOS Finder ("Kopiera", "Flytta", "Radera"). Trash variant verb "Flytta till papperskorgen" / gerund "Flyttar till
  papperskorgen". `high`.
- **overwrite / skip / rename / merge (conflict policies): `skriv över` / `hoppa över` / `byt namn` / `slå samman`** ·
  "skriv över" (style.md), "hoppa över" (Nautilus "\_Hoppa över"), "byt namn" (macOS "Byt namn på…"), "slå samman"
  (Nautilus "Sammanfoga", but "slå samman" reads more natural for folder-merge UI). `high` except merge `tentative`
  (chose "slå samman" over Apple/GNOME "sammanfoga" for plainer voice).
- **rollback (undo a partial transfer): `återställ` (button) / `återställning` (noun) / `återställer` (in progress)** ·
  macOS uses "ångra" for undo, but Cmdr's rollback is "delete the partial files and revert", so "återställ" (restore)
  fits better than "ångra". `tentative` (Cmdr-specific sense; review).
- **target (of a symlink / conflict): `mål`** · "målet", "målmapp", "målvolym", "målsökväg". macOS/MS standard. `high`.
- **merge (no-op) / "under cursor": `under markören`** · "markör" = cursor (macOS "markören"). `high`.
- **source-available: `källtillgänglig`** · composed (källa + tillgänglig), parallel to "open source" → "öppen källkod";
  no direct source. `tentative` (composed; review).
- **provider (AI/cloud): `leverantör`** · MS ("leverantör"). "molnleverantör" for cloud provider. `high`.
- **endpoint (URL): `slutpunkt`** · MS terminology ("slutpunkt"). "Slutpunkts-URL". `high`.
- **API key: `API-nyckel`** · MS ("nyckel" for key); keep "API" verbatim, hyphenate the compound. `high`.
- **onboarding (wizard): `kom igång` / `guiden`** · no single Swedish noun for "onboarding"; framed as "Kom igång med
  Cmdr" (title) and "guiden" (the wizard). `tentative` (descriptive framing).
- **feedback: `återkoppling`** · MS terminology ("återkoppling"). "Skicka återkoppling". `high`.
- **dir (abbrev. of directory in tight scan stats): `kat.`** · abbreviation of "katalog" (style.md katalog entry), kept
  with a period to read as a clipped unit next to a live count, mirroring English "dir". `tentative` (abbreviation
  convention; review for clarity vs. spelling out "kataloger").

UI section names captured (volume-switcher group headings; keep consistent across files): Favoriter (Favorites), Volymer
(Volumes), Moln (Cloud), Mobil (Mobile), Nätverk (Network). File-list columns: Namn, Tillägg, Storlek, Ändrad, Skapad,
Git.

From the `licensing.json` + `ai.json` + `viewer.json` pass (2026-06-21). Reuses all terms above (provider →
`leverantör`, endpoint → `slutpunkt`, API key → `API-nyckel`, delete → `radera`, encoding follows below); new ones:

- **license: `licens`** · MS terminology ("licens"). "Licensnyckel" (license key), "Licenstyp" (license type),
  "Licensinformation" (license details). `high`.
- **commercial / personal / perpetual (license tiers): `kommersiell` / `personlig` / `evig`** · MS ("kommersiell",
  "evig" for perpetual); "personlig" for the Personal tier. "Kommersiell prenumeration", "Kommersiell evig". `high`.
- **subscription: `prenumeration`** · MS terminology ("prenumeration"). `high`.
- **activate / deactivate (a license): `aktivera` / `inaktivera`** · MS, macOS ("Aktivera"). "Aktivera" the key;
  reset/deactivate framed as "inaktiverar din nuvarande licens". `high`.
- **renew: `förnya`** · MS terminology ("förnya"). "Förnya licens". `high`.
- **expire / expired: `gå ut` / `gick ut`** · natural Swedish for a lapsed license ("Din licens har gått ut", "Gick ut
  den {date}"). MS uses "upphöra att gälla"; "gå ut" reads warmer and shorter. `high`.
- **valid / validity: `giltig` / `giltighet`** · MS ("giltig", "giltighet"). "Giltig till {date}". `high`.
- **verify (a license/download): `verifiera`** · MS terminology, macOS. Aligns with style.md. `high`.
- **viewer (file viewer): `förhandsvisning`** · per style.md viewer entry; the window/feature noun. "Filförhandsvisning"
  (screen-reader heading), "Förhandsvisningsåtgärder" (context menu). `high`.
- **encoding (text/character encoding): `teckenkodning`** · macOS/Nautilus ("teckenkodning"); MS's bare "Encoding" is a
  generic-protocol sense, so prefer the standard Swedish file-encoding compound. `high`.
- **western (encoding group): `västerländsk`** · standard Swedish for the Western/Latin legacy encodings group.
  `tentative` (no direct UI source; convention).
- **line(s) (text line in viewer): `rad` / `rader`** · macOS/standard. Plural "rader"; "radnummer" (line numbers),
  "radbrytning" for word wrap (style.md). `high`.
- **character(s): `tecken`** · MS terminology ("tecken"). Neuter, plural unchanged ("tecken"). `high`.
- **clipboard: `urklipp`** · per fileExplorer pass; "i urklipp" (on the clipboard), "urklippsgränsen" (clipboard limit).
  `high`.
- **selection (selected text/region): `markering`** · macOS ("markering"); "Spara markering", "Markeringen sparades".
  `high`.
- **reload (re-read a changed file): `läs in på nytt`** · macOS/MS framing; "Läs in på nytt" (button), distinct from
  "uppdatera" (refresh a list). `high`.
- **loading: `läser in…`** · macOS Finder ("Läser in…"). `high`.
- **streaming (large-file viewer mode): `strömma` / `strömningsläge`** · standard Swedish IT ("strömma"). `tentative`
  (no direct file-viewer source; convention).
- **tail (follow-file mode): `Tail`** · kept verbatim as the Unix `tail -f` term (the toggle label, aria, and hint all
  reference it); no natural Swedish equivalent that stays recognizable. `tentative` (loanword kept by design).
- **runtime (AI runtime bundle): `körtid`** · MS terminology ("körtid"). `high`.
- **model (AI model): `modell`** · MS ("modell"). "Modellnamn", "AI-modell". `high`.
- **memory (RAM): `minne`** · macOS Get Info ("Minne:"), MS ("minne"). "Minnesvarning", "minnesanvändning". `high`.
- **request (API request): `förfrågan`** · MS terminology; "Förfrågan nådde tidsgränsen". `high`.
- **quota: `kvot`** · MS terminology ("kvot"). `high`.
- **detected (auto-detected encoding): `upptäckt`** · "{label} (upptäckt)"; lowercase inside the parenthetical. `high`.
- **apply (a setting): `tillämpa`** · MS terminology ("tillämpa"). The context-size Apply button. `high`.
- **rate-limit: `hastighetsbegränsa`** · composed standard IT term (hastighet + begränsa); no direct macOS source.
  `tentative` (composed; review).

Settings section reference reused: AI section path "Inställningar > AI" (Inställningar per style.md). Brand/format
values kept verbatim and thus identical to English: Cmdr, GitHub, Discord, PDF, Unicode, Regex, Server, Status, System,
Text (Swedish cognate), and pure-placeholder values ({width} × {height}).

From the `queryUi.json` + `commands.json` pass (2026-06-21). The search/query UI and the command palette + app/menu
command labels. Reuses all terms above; new ones:

- **command palette: `kommandopaletten`** · composed standard term (kommando + palett); no direct macOS UI source. Verb
  context "Öppna kommandopaletten", "Stäng paletten". `tentative` (composed; matches the app's established UI name).
- **get info (macOS): `Visa info`** · macOS Finder "Get Info" → "Visa info" (verified in pile `sv/macOS/Finder`). The
  non-macOS twin "File properties" → "Filegenskaper" (MS "egenskaper"). `high`.
- **show in Finder (Reveal): `Visa i Finder`** · macOS Finder "Reveal" → "Visa i Finder" (pile). Non-macOS twin → "Visa
  i filhanteraren". `high`.
- **zoom (UI text size): `zooma` (verb) / `zoom` (noun)** · macOS AppKit "Zoom" → "Zooma" (pile). "Zooma in/ut", "Zooma
  till 100 %", reset toast "Zoom återställd". Percent with a space before % per Swedish typography ("100 %"). `high`.
- **context menu: `snabbmeny`** · macOS/Swedish standard for the right-click menu (AppKit "snabbmeny"); MS's
  "kontextmeny" is the literal alt. `high`.
- **Hide / Hide others / Show all (macOS app menu): `Göm` / `Göm andra` / `Visa alla`** · macOS AppKit app-menu
  conventions ("Göm <app>", "Visa alla" in the pile). `high`.
- **quit (macOS app menu): `Avsluta`** · macOS AppKit ("Avsluta <app>"). `high`.
- **scope (search-in folder limit): `omfattning`** · MS terminology ("omfattning") for scope; the chip label itself is
  "Sök i" (Search in). `high`.
- **case-sensitive: `skiftlägeskänslig`** · standard Swedish IT term (skiftläge = letter case). `high`.
- **ascending / descending (sort): `stigande` / `fallande`** · macOS Finder ("stigande/fallande ordning", pile), Thunar.
  `high`.
- **byte (size unit): `byte`** · the Swedish word is also "byte", invariant in plural (1 byte / 2 byte), so the plural
  unit differs from English "bytes". `high`.
- **wildcard: `jokertecken`** · standard Swedish IT term for `*`/`?` wildcards. `high`.
- **onboarding (the wizard): `introduktion` / `introduktionsguide`** · natural Swedish for guided first-launch setup; no
  macOS source. `tentative` (composed; review).
- **What''s new: `Nyheter`** · standard Swedish app-menu term for the release-notes view. `high`.

Brand/technical values kept verbatim and thus identical to English: Cmdr, macOS, Finder, Regex, Glob, AI, and
pure-placeholder values ({mode} · {age}, {prefix} {valueText} {unit}, etc.).

- **Quick Look -> `Överblick`** · macOS Swedish · `high`. The localized Apple FEATURE name, not a brand kept verbatim:
  Apple translates "Quick Look" to "Överblick" in Swedish Finder (pile `sv/macOS/Finder/LocalizableMerged.json` key
  `TL14` = "Överblick"; the verb form "Överblicka" appears in keys `N169.17`/`N169.18`/`N169.20`). So Cmdr uses the term
  the user sees in their own Finder. Applied to `commands.fileQuickLook.mac.label` and the three `settings.json`
  Quick-Look mentions. The generic "quick preview"/"quick view" descriptors in `fileExplorer.quickLookHint.*` stay
  generic ("snabbtitt"), mirroring the EN source's deliberate non-feature-name wording there.

- **Keychain (the credential store) -> `Nyckelring`; Keychain Access (the app) -> `Nyckelhanterare`** · macOS Swedish ·
  `high`. The localized Apple FEATURE name, not a brand kept verbatim (same Decision-1 principle as Quick Look above;
  see `docs/guides/i18n-translation.md` § Term-choice principles). Apple localizes both: the store noun is "Nyckelring"
  (definite "nyckelringen"), the app is "Nyckelhanterare" (verified in
  `/System/Library/CoreServices/Applications/Keychain Access.app/Contents/Resources/sv.lproj` — `InfoPlist.loctable`
  `CFBundleDisplayName` = "Nyckelhanterare"; `Localizable.loctable`/`MainMenu.loctable` use "Nyckelring"/"nyckelringen"
  throughout). Applied per sense: the store noun for "macOS Keychain denied access" → "macOS Nyckelring"
  (`ai.secretError.keychainTitle`), "Remember in Keychain" → "Kom ihåg i nyckelringen", "Remove saved password from
  Keychain" → "…från nyckelringen", "allow Keychain access" → "åtkomst till nyckelringen"; the app name for "Open
  Keychain Access" → "Öppna Nyckelhanterare" (`ai.secretError.keychainBody`). Supersedes the old "keep Keychain
  verbatim" note. Not on the enforced don't-translate brand list.

From the `indexing.json` + `downloads.json` + `errorReporter.json` + `shortcuts.json` + `mtp.json` + `ui.json` pass
(2026-06-21, wave 1 batch 2). Reuses all terms above; new ones:

- **download (the macOS folder): `Hämtade filer`** · macOS Finder shows the Downloads folder as "Hämtade filer". Used
  for "your Downloads folder" / "Go to Downloads". The action verb stays `hämta`, the noun `hämtning(ar)` (style.md
  download entry). `high`.
- **jump to (a file/download): `hoppa till`** · natural Swedish for the "jump"/reveal-and-select action ("Hoppa till
  filen", "hoppa till din senaste hämtning"). `tentative` (composed; low risk).
- **global (system-wide shortcut): `global` / `globalt`** · MS terminology ("global", adjective); "global genväg" for
  the system-wide hotkey, adverb "globalt" ("Hoppa med {key} globalt"). Kept the cognate; identical to English at the
  bare scope-label "Global". `high`.
- **shortcut (keyboard): `genväg`** · standard Swedish (macOS also "kortkommando"); "Tangentbordsgenvägar" for the
  section (style.md), "genväg" for an individual binding. `high`.
- **modifier (modifier key): `modifierare`** · "Lägg till en modifierare (⌘, ⌃, ⌥ eller ⇧)". macOS pile lacks the term;
  MS's "låstangent" is the wrong (lock-key) sense, so chose the standard Swedish "modifierare". `tentative` (no direct
  macOS source; MS sense rejected).
- **register / registered (a global hotkey with the OS): `registrera` / `registrerad`** · MS terminology ("registrera").
  "Registrerad" / "Inte registrerad" status; "Det gick inte att registrera: …" for the calm failure. `high`.
- **combo (key combination): `kombination`** · natural Swedish; "Välj en annan kombination", "ogiltig kombination".
  `high`.
- **notification / toast: `avisering`** · per settings glossary (MS/macOS "avisering"); "Avfärda avisering", "Gör den
  här aviseringen mer kompakt". `high`.
- **dismiss: `avfärda`** · toast/alert dismiss button. MS gives "stäng"; chose "avfärda" to distinguish dismissing a
  notification from closing a dialog ("Stäng"). `tentative` (MS says "stäng"; "avfärda" reads clearer for a toast).
- **error report: `felrapport`** · standard Swedish compound (fel + rapport; MS "rapport"). "Skicka felrapport". The
  dialog stays calm, no bare "fel" as a status label. `high`.
- **redact / scrub (privacy-strip logs): `maskera` / `rensa bort`** · "Loggarna maskeras lokalt", "… rensas bort innan
  de skickas", "efter maskering". MS's "redact → redigera" is the wrong sense; "maskera/rensa bort" is the standard
  privacy framing. `tentative` (MS sense rejected; composed from the privacy domain).
- **reference ID: `Referens-ID`** · composed (referens + ID); keep "ID" verbatim, hyphenate. `high`.
- **manifest: `manifest`** · MS terminology ("manifest", neuter); identical to English. `high`.
- **note (free-text note in a form): `notering`** · macOS/standard ("notering"). "Lägg till en notering". `high`.
- **preview (of what will be sent / dialog preview): `förhandsvisning`** · per style.md viewer entry; "Förbereder
  förhandsvisning…". MS's first sense ("applatshållare") is wrong. `high`.
- **suggestion(s) (combobox): `förslag`** · macOS AppKit ("Förslag", "Förslagsfönster"). "Visa förslag", "Läser in
  förslag". `high`.
- **options (generic popover label): `Alternativ`** · macOS Finder ("Alternativ", key N280). `high`.
- **select (dropdown placeholder): `Välj…`** · macOS standard. `high`.
- **udev / USB / Terminal / ptpcamerad / Android / Linux: verbatim** · device/OS/process names kept literal per the
  do-not-translate set; "USB-enhet", "MTP-enhet", "udev-regler" hyphenate the compound. `high`.
- **camera daemon / system daemon: `kameradaemon` / `systemdaemon`** · "daemon" is the standard Swedish IT loanword;
  compound with the qualifier. `tentative` (loanword by convention).
- **exclusive access: `exklusiv åtkomst`** · MS/standard ("exklusiv", "åtkomst"). `high`.
- **scan through / rescan (drive index): `söka igenom` / `genomsökning`** · per fileExplorer glossary; "Söker igenom din
  enhet…", "Gör en ny genomsökning". `high`.
- **entries (scanned filesystem entries): `poster`** · standard Swedish ("post" = record/entry, plural "poster").
  `high`.
- **events (replayed change events): `händelser`** · macOS/standard ("händelse"). "{n} händelser bearbetade". `high`.

macOS feature names kept verbatim (brand, shown in shortcut-conflict warnings): Spotlight, Mission Control, Spaces.
macOS feature names translated to Apple-standard Swedish (not in this pile's macOS bundle, flag for native review):
Character Viewer → `Teckenvisare`, Force Quit → `Avsluta tvingat`, App windows → `Appfönster`, Finder search window →
`Finders sökfönster`. `tentative`. Brand/format/cognate values kept verbatim and thus identical to English: macOS, Cmdr,
MTP, USB, OK, App, Global, Manifest, and pure-placeholder values ({currentText} / {maxText}).

From the small-files pass (`crashReporter` + `downloads` + `errorReporter` + `whatsNew` + `updates` etc.). These terms
were settled during translation from direct reference-pile hits but not recorded at the time; captured here so future
passes stay consistent:

- **crash report: `kraschrapport`** · standard Swedish compound (krasch + rapport); MS "rapport". Used in
  `crashReporter.json` + `settings.json`. `high`.
- **changelog: `ändringslogg`** · standard Swedish IT compound (ändring + logg). Used in `settings.json` +
  `whatsNew.json`. `high`.
- **restart (the app): `Starta om`** · macOS AppKit ("Starta om"), MS. The imperative on restart prompts; used across
  `errors.json`, `onboarding.json`, `settings.json`, `updates.json`. `high`.

From the transfer-queue pass (`queue.json` + the new pause/queue/background keys in `fileOperations.json` +
`commands.json`). The standalone transfer-queue window with pause/resume/cancel and send-to-background controls. Reuses
the copy/move/delete verbs above; new ones:

- **pause: `pausa` (verb/button) / `pausad` (status)** · macOS Finder shows "Pausa" and "Pausad" for a paused copy
  ("Kopiering av ”…” har pausats"). Button "Pausa", status word "Pausad". `high`.
- **resume: `återuppta`** · macOS Finder ("Återuppta kopiering"), Total Commander ("Återuppta avbruten överföring"). The
  button that restarts a paused transfer. `high`.
- **queue (the transfer queue): `kö`; transfer queue `överföringskö`; queued status `Väntar`** · Total Commander uses
  the bare noun "Kö" for its job queue; Thunar renders "Job queued" as "Jobb köade" (verb "köa"). The window noun is
  "överföringskö" (compound överföring + kö, definite "överföringskön"); the per-row queued state reads "Väntar"
  (waiting its turn). The toolbar "Queue" button (send-to-background) on the progress dialog is the bare noun "Kö".
  `high`.
- **background / send to background: `i bakgrunden` / `skicka till …kön`** · Total Commander ("…överföringar i
  bakgrunden", "i bakgrunden"). "Keep this running in the background" → "Håll igång den här i bakgrunden"; "Send to the
  transfer queue" → "Skicka till överföringskön" (sending to the queue IS sending to the background here). `high`.
- **transfer-row gerunds (queue row label): reuse `Kopierar` / `Flyttar` / `Raderar` / `Flyttar till papperskorgen`;
  fallback `Arbetar`** · same select branches as `fileOperations.transferProgress.titleActive`, no trailing ellipsis
  (it's a row label, not a title). "other {Working}" → "Arbetar". `high`.
- **"Couldn''t finish" (failed-row status): `Gick inte att slutföra`** · the calm wording for a failed transfer, no bare
  "fel"/"misslyckades" (style.md). `high`.

## Cross-file consistency reconciliation (post-fanout review, 2026-06-21)

The per-file fan-out left a few same-term-rendered-differently drifts; resolved across all `sv` files:

- **Ellipsis: mirror the EN source per key.** EN is itself mixed (ASCII `...` for in-progress/placeholder text, Unicode
  `…` for menu-item labels), so the faithful and now-uniform rule is: each `sv` value uses the SAME ellipsis character
  its EN source uses. 56 keys that had been "upgraded" to `…` where EN used `...` were reverted; a space-before-ellipsis
  quirk in 7 `settings.json` keys (`Anpassat ...`) was removed. Don't blanket-convert to `…`.
- **feedback → `återkoppling` everywhere.** `commands.feedbackSend.label` had drifted to the loanword `feedback`;
  aligned to the glossary's `återkoppling` (matches `feedback.json`, `onboarding.json`).
- **"What''s new" feature name → `Nyheter`.** The `settings.json` internal description referred to the popup as
  `”Vad är nytt”`; aligned to the feature's actual name `Nyheter` (the dialog title is "Nyheter i Cmdr").
- **Swedish quotes `”…”`, never straight `"…"`.** `commands.handler.favoriteAdded` used ASCII quotes around `{name}`;
  fixed to `”{name}”` (and the verb to the standard past tense `Lade till`, matching `hostRemoved` → "Tog bort").
- **Cmdr genitive: `Cmdrs`** (no apostrophe, Swedish rule), compounds hyphenated (`Cmdr-loggar`, `Cmdr-guld`). The
  `desktop-i18n-dont-translate` check flags `Cmdrs` as a "dropped Cmdr token" (boundary matcher); this is a known false
  positive shared with `hu`/`fr`, NOT a defect: the brand IS present, inflected correctly. Don't "fix" it to satisfy the
  check.

## Navigation & file ops keys re-validated against the reference pile (2026-06-26)

The `settings.json` + `fileExplorer.json` double-click-to-parent and breadcrumb keys, first translated glossary-only,
re-checked against `sv/macOS/`. New term:

- **parent folder / enclosing folder: `överordnad mapp`** (definite `den överordnade mappen`) · macOS Finder, confirmed
  (was `tentative`). Finder uses it uniformly: "Go To Enclosing Folder" → "Öppna överordnad mapp", "Navigates the front
  Finder window to its enclosing folder" → "Navigerar det översta Finder-fönstret till den överordnade mappen", "Reveal
  in enclosing folder" → "Visa i överordnad mapp", and standalone titles "Överordnad mapp"
  (`sv/macOS/Finder/LocalizableMerged.json` keys `N162`, `FV10`, `FV9`, `300753.title`, `250.title`, `BU37_V1/V2`). The
  first pass's `överordnad mapp` was right; upgraded `tentative` → `high`. **`upp till`** for "go up to" in the helper
  text stays (natural Swedish, no competing source).
- **go up a folder / navigate to (the gesture): `gå upp till den överordnade mappen`** · the shortened toggle label
  "Double-click the pane background to go up a folder" → "Dubbelklicka på panelens bakgrund för att gå upp till den
  överordnade mappen". "go up a folder" = go to the parent, so it reuses `överordnad mapp`; "gå upp till" is the natural
  Swedish for going up a level (the same phrasing Finder uses in body strings). For Finder's imperative menu COMMAND the
  form is "Öppna överordnad mapp"; the descriptive sentence "Navigerar … till den överordnade mappen" is also attested.
  The breadcrumb tooltip "Click to navigate to {path}" keeps the warmer "Klicka för att gå till {path}". `high`.
- **file row (a row in the file list): `filrad`** (definite `filraden`) · row = `rad`, from KDE Dolphin "Highlight
  entire row" → "Markera hela raden" (`sv/kde-dolphin/dolphin.po`); compounded with `fil` per the standard Swedish IT
  pattern. Toggle description "That''s the empty space around the file list, not a file row." → "Det är den tomma ytan
  runt fillistan, inte en filrad." (reuses settled `tomma ytan` + `fillista` → definite `fillistan`). `high`.
- **What just happened? (one-time hint title): `Vad hände nyss?`**; notification body "This navigates to the parent
  folder" → "Det tar dig till den överordnade mappen" (warmer notification voice). `high`.
- preset (value in a settings-picker dropdown) → förinställning; "back to presets" → "Tillbaka till förinställningar" ·
  pile adjective "förinställd/förinställda" (shared root), macOS SV print dialog "Förinställningar" · high

From the FAT32-size-guard pass (`errors.write.filesTooLargeForFilesystem.*` +
`fileOperations.errorDialog.tooLargeAndMore`). The copy/move error when a file exceeds a FAT32 drive's ~4 GB cap. Reuses
`enhet` (drive), `fil/filer`. New ones:

- **too large (for a drive): `för stor` / `för stora`** · macOS ("för stor"/"för stora", pile). Agrees with the noun:
  "Filen är för stor", "Vissa filer är för stora". `high`.
- **formatted as/with (a filesystem): `formaterad med {format}`** · this file's own precedent
  (`errors.listing.notSupportedErrno.suggestion`: "kan den vara formaterad med ett filsystem som har begränsningar …
  FAT32 inte lagra filer större än 4 GB") + macOS Disk Utility (Skivverktyg) "Formatera"/"formaterad"; FAT32 and exFAT
  are filesystem-format names kept verbatim (task + the format-menu list in `sv/macOS`). Chose `med` over `som` to match
  the existing in-file phrasing. `high`.
- **larger than: `större än`** · macOS Spotlight criteria ("är större än", pile, 8 hits). "lagra filer större än
  {maxSize}" reuses the exact `notSupportedErrno` phrasing already in this file. `high`.
- **no such limit: `ingen sådan gräns`** · `gräns` = limit (style.md/MS); natural Swedish. "som inte har någon sådan
  gräns". `high`.
- **and N more (files) (trailing "+N" line under a truncated list): `och ytterligare {countText} {fil/filer}`** ·
  composed natural Swedish; `ytterligare` = additional/more, front-loaded so no trailing word is needed. ICU plural
  one→`fil`, other→`filer`. `high` (compound by convention; low risk).
- preset (value in a settings-picker dropdown) → förinställning; "back to presets" → "Tillbaka till förinställningar" ·
  pile adjective "förinställd/förinställda" (shared root), macOS SV print dialog "Förinställningar" · high

From the dialog-polish pass (2026-06-30; new `fileOperations.json` field labels + scan-spinner tooltips). Reuses
scan/genomsökning terms above; new ones:

- **Action (what a control chooses; screen-reader label `transferDialog.operationAria`): `Åtgärd`** · macOS Finder
  ("Åtgärd", standalone label) and MS terminology both render action → "åtgärd"; matches the glossary's
  `åtgärden {verb}` framing. `high`.
- **Scanning… (tooltip + SR label on the counting spinner): `Söker igenom…`** · matches this file's
  `transferProgress.stageScanning` ("Söker igenom") and the glossary `genomsökning` / `söker igenom` scan-pass entries.
  Unicode ellipsis mirrors the EN source per the ellipsis rule. `high`.
- **"doesn''t exist yet … will create it during the copy/move" (yellow inline warning under the destination box):
  `finns inte än` + `Cmdr skapar den under {kopieringen|flytten}`** · "doesn''t exist" → `finns inte` (Total Commander
  "Katalogen … finns inte. Vill du skapa den?"), warmed with `än` (yet); created actively (`Cmdr skapar den`, active
  voice over the pile's passive `skapas`). The operation noun is definite: `under kopieringen` (attested copy-noun,
  pile) for copy, `under flytten` (definite of this file's settled `flytt` move-noun) for move. Two literal sentences,
  no ICU select, per the operation-specific keys. `high` (move-noun definite `flytten` regular but not directly
  attested; `flyttningen` is the pile alt).
- **queue.row.label progress arms (rename / create folder / create file)** · `Byter namn` / `Skapar mapp` / `Skapar fil`
  · present-tense style of the sibling arms (Kopierar, Flyttar); Nautilus ("Byter namn", "Skapar"), settled `byt namn`,
  `mapp`/`fil` · high

From the archive-browsing pass (2026-07-05; the 27 archive keys + the new `archive_edit` queue arm). Cmdr can now step
INTO a zip/tar/7z the way it steps into a folder, and offers browse/open/ask on Enter. New terms:

- **archive (the compressed file: zip/tar/7z, browsed like a folder): `arkiv`** (neuter: ett arkiv, definite `arkivet`,
  plural unchanged `arkiv`) · macOS Finder authoritative: "Komprimerar objekt till ett arkiv" (Compressing items into an
  archive), "Välj ett lösenord för arkivet", "Flytta arkiv till"; Total Commander (Cmdr's two-pane lineage) uses
  "arkivfil"/"arkiv" throughout and even has the exact browse-like-a-folder concept ("dubbelklicka på arkivfilen som på
  en mapp"). The bare menu label "Arkiv" = the macOS **File** menu, but in every archive/zip context Apple itself uses
  "arkiv" for the compressed file, so no collision in Cmdr's surfaces. `high`.
- **zip archive: `zip-arkiv`** · macOS Finder exact term ("Zip-arkiv", "ZIP-arkiv", "Zip-arkivformat"; same pattern as
  "CPIO-arkiv", "Apple-arkiv"). The `.zip` extension token stays verbatim; the format word lowercases in the compound
  (`zip-arkiv`, `zip-fil`). `high`.
- **read-only archive: `skrivskyddat arkiv`** · `skrivskyddad` (glossary read-only) + neuter agreement on `arkiv` (`-t`
  → `skrivskyddat`). `high`.
- **bundle / app bundle: `paket` (generic bundle) / `appaket` (app bundle)** · macOS = "paket" ("Visa paketets innehåll"
  = Show Package Contents, the Finder term for a bundle/app). Generic "bundle" (keys `archiveEnterMenu.ariaLabel`,
  `enterBehavior.label`) → `paket`; "App bundles" (the card/section grouping .app/.bundle/.framework) → `appaket` (app +
  paket, Swedish three-p reduction: appp→app). Faithfully mirrors EN's own split ("bundles" vs "app bundles"). `appaket`
  is a convention-composed compound (macOS-backed `paket`, not directly attested as a compound), so `tentative` (review
  whether `appaket` reads cleanly vs. `programpaket`).
- **browse (step inside like a folder): `bläddra`; "browse like a folder" → `bläddra som en mapp`** · macOS "Bläddra i
  listvy/kolumnvy", "bläddra i ditt filsystem"; TC "…som på en mapp". Short segmented-control cell "Browse" → `Bläddra`.
  `high`.
- **extract (unpack an archive): `extrahera`** · the explorer family overwhelmingly (Nautilus/Thunar/Dolphin, 17+ hits)
  uses "extrahera"/"extraherad"; TC's "packa upp" is the two-pane alt. Chose `extrahera` for the macOS/explorer voice.
  `high`.
- **open with default app: `öppna i standardappen`** · matches the EXISTING sv catalog
  (`fileExplorer.quickLookHint.enterOpens`: "öppna filer i standardappen"); Thunar's "standardprogram" is the alt, but
  Cmdr's voice uses "app" (76 catalog hits vs. "program"). `high`.
- **configure (opens Settings): `Konfigurera…`** · macOS/MS ("Konfigurera"); trailing Unicode ellipsis kept (signals a
  window opens). `high`.
- **ask (Enter-behavior option, segmented cell): `Fråga`** · macOS "Fråga …" prompt convention. `high`.
- **"for good" / permanently (delete finality): `permanent`** · macOS uses "permanent" (14 hits) for irreversible
  removal. Archive-delete warning: "There''s no trash inside an archive." → "Det finns ingen papperskorg i ett arkiv." +
  "…removed from the zip for good." → "Objekten tas bort permanent ur zip-arkivet." (`ta bort … ur` = remove out of the
  container, glossary's list/collection sense; `ur` matches TC's "ta bort … ur arkivfilen"). `high`.
- **archive_edit (queue.row.label arm, "Editing archive"): `Redigerar arkiv`** · present-tense sibling-arm style
  (Kopierar, Flyttar); `redigera` = edit (glossary, macOS). Inserted before the `other` arm; sourceHash set to
  `9f18acf`. `high`.

From the paste-clipboard-as-file pass (2026-07-07; the 5 `settings.fileOperations.pasteClipboardAsFile.*` keys + 2
`fileExplorer.clipboard.pastedAsFile*` keys). What ⌘V does in a folder when the clipboard holds text/an image/a PDF
instead of copied files. Reuses `klistra in` (paste), `urklipp` (clipboard), `Skapa`/`byt namn`, `Inställningar`. New
ones:

- **paste (verb), pasted (the toast, past tense): `klistra in` / `klistrade in`** · macOS AppKit ("Klistra in"),
  Nautilus ("Klistra in", and "Pasted image" → "Inklistrad bild"). The confirmation toast uses the active past tense
  "Klistrade in … som {filename}" (active voice over Nautilus's adjectival "Inklistrad"). `high`.
- **clipboard content: `urklippsinnehåll`** · `urklipp` (glossary clipboard) + `innehåll` (content; MS "Innehåll",
  Nautilus). Attested `urklipps-` compound pattern in Nautilus ("Urklippssträng", "urklippsdata"). Settings label
  "Klistra in urklippsinnehåll som en fil". `high` (compound by attested pattern).
- **do nothing (radio option): `Gör ingenting`** · natural Swedish; no direct UI source (no "Do Nothing" behavior option
  in the pile). `tentative` (composed; low risk, unambiguous).
- **create file / create and rename (radio options): `Skapa fil` / `Skapa och byt namn`** · `Skapa` (macOS/catalog
  "Skapa ny fil", "Skapa mapp") + settled `byt namn`. `high`.
- **"Pasted clipboard {image/PDF/text} as {filename}" (info toast):
  `Klistrade in {en bild|en PDF|text} från urklipp som {filename}`** · the `{kind}` select branches carry the article
  per phrase (image → "en bild", pdf → "en PDF", text → bare mass noun); "från urklipp" (from the clipboard) renders the
  "clipboard" modifier uniformly across all three branches (compounding urklipps+bild/PDF/text wouldn't read cleanly).
  `{filename}` is uncontrolled, so the sentence ends on it and reads correctly for any value. `high`.

## Archive-password dialog (2026-07-08)

Terms settled while translating the encrypted-archive unlock modal (`fileOperations.archivePassword.*`; macOS AppKit +
Total/Double Commander sv).

- password-protected → `lösenordsskyddad` · TC/DC sv phrasing · high. Body: "… är lösenordsskyddad."
- password (noun) → `Lösenord` · macOS/MS · high. Input aria-label compounds to `Arkivlösenord`.
- unlock (button + verb) → `Lås upp` · macOS AppKit ("Lås upp") · high. Verb "för att låsa upp den".
- archive → `arkiv` · settled sv glossary · high.
- COMMON GENDER: `arkiv` is treated common-gender here, so the predicate adjective takes the -ad/en-word form
  `lösenordsskyddad` (not neuter `-skyddat`) and the pronoun is `den` ("låsa upp den"), not `det`.

Settled while translating the Compress feature:

- compress (verb / control label) → `Komprimera` · Finder `sv/macOS` ("Komprimera", `Compress ${sources}` → "Komprimera
  ${sources}") · high. Used for `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and both title-verb
  branches.
- compressing (progress form) → `Komprimerar` · derived on the sibling `Kopierar`/`Flyttar` · high. `scanTitleCompress`
  = "Verifierar före komprimering...".
- compressed (result toast) → `Komprimerade` (past tense) · mirrors `transfer.split.clean` ("Kopierade {phrase}") ·
  high.
- replace (overwrite warning) → `ersätter` · Finder `Replace` → "Ersätt" · high.
- archive (name) → `arkiv`/`Arkivets` · Finder `Zip archive` → "Zip-arkiv" · high. `.zip` in straight double quotes.
- compression level (slider label) → `Komprimeringsnivå` · TC `sv` "Komprimering (0-9)" + `nivå`; standard sv term
  `Komprimeringsnivå` · high. `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `Snabbare` · TC `sv` "Snabbast komprimering (1)" (root `snabb`) · high. Marks
  quicker packing, not app speed. `.faster`.
- smaller (slider high end, level 9) → `Mindre` · pairs with `Snabbare`; marks the smaller output file (TC `sv` high end
  "Maximal komprimering") · high. `.smaller`.
- No `sameAsSourceJustification` needed: all values differ from English.

From the Operation-log pass (2026-07-09; `operationLog.json` + the two `commands.logOperationLog.*` keys). The alpha
dialog listing recent file operations (copy/move/delete/rename/…) with per-op rollback. Reuses the transfer verbs
(`Kopierade`/`Flyttade`/`Raderade`/`Komprimerade`/`Bytte namn på`), the queue-status words, and the rollback family; new
ones:

- **operation log (the feature/dialog): `Åtgärdslogg`** · reuses the ALREADY-SHIPPED `settings.section.operationLog` =
  "Åtgärdslogg" in `sv/settings.json` (åtgärd = action/operation per the `Åtgärd:` field-label entry + MS/macOS; logg =
  log). Applied to `operationLog.dialog.title` and `commands.logOperationLog.label` so the command, the settings
  section, and the dialog title all read the same word. `high`.
- **operation (a logged file operation): `åtgärd`** (definite `åtgärden`, plural `åtgärder`) · matches
  `settings.operationLog.*` ("loggade åtgärder", "gå igenom din historik") and the `åtgärden {verb}` framing. `high`.
- **history (operation history): `historik`; "operation history" → `åtgärdshistorik`** · `settings.operationLog` uses
  "historik"/"Behåll historik i"; compounded åtgärd+historik for `loadError`. `high`.
- **roll back / rollback (reverse a logged operation): reuse `återställ`/`återställa`/`återställer`/`återställd`** · the
  settled rollback family (glossary rollback entry + `fileOperations.transferProgress` "Återställer"/"Återställ").
  Status chips: notRollbackable → "Går inte att återställa", rollbackable → "Går att återställa", rollingBack →
  "Återställer", rolledBack → "Återställd", partiallyRolledBack → "Delvis återställd". Command description "roll them
  back" → "återställ dem". `high`. NOTE: `settings.operationLog.intro` (already shipped) phrases the same concept as
  "ångra åtgärder"; the dialog uses the `återställ` family for consistency with the transfer-rollback surface — flagged
  for David if he wants the intro aligned.
- **status chips (reuse queue.row.status): queued → `Väntar`, running → `Pågår`, done → `Klar`, canceled → `Avbruten`,
  "Didn''t finish" → `Gick inte att slutföra`** · matched exactly to `queue.json` `queue.row.status`. `high`.
- **initiator/provenance labels: You → `Du`, AI client → `AI-klient`, Agent → `Agent`** · `du` address (style.md); MS
  "klient" hyphenated `AI-klient`; "agent" is the same word in Swedish (`agenten` across `queryUi`/`onboarding`), so
  `Agent` carries a `sameAsSourceJustification`. `high`.
- **per-item outcome "Skipped": `Överhoppad`** · adjectival participle of `hoppa över` (the settled skip verb), matching
  the participle style of the sibling outcomes (`Klar`, `Återställd`). `tentative` (participle form not directly
  attested; the verb `hoppa över` / "hoppade över" is — review whether `Överhoppad` or `Hoppade över` reads better as a
  one-word chip).
- **load / load more: `Läs in` / `Läs in 50 till`** · `läsa in` (glossary loading/reload); "50 till" = 50 more. `high`.
- **more items (ICU plural tail): `och ytterligare {countText} objekt`** (both branches; `objekt` neuter invariant) ·
  reuses `fileOperations.errorDialog.tooLargeAndMore` "och ytterligare {countText}" pattern. `high`.
- **recorded items: `registrerade objekt`** · `registrera`/`registrerad` (glossary register entry) + `objekt`. `high`.

## Ask Cmdr pass (2026-07-13; `askCmdr.json` + the `settings.askCmdr.*`/`settings.advanced.logLlmCalls.*`/

`settings.section.askCmdr`/`commands.askCmdrToggle.*` keys)

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
- **request (a tool call the assistant asked to make): `förfrågan`** · reused from the existing glossary entry (API
  request). `askCmdr.tool.refused` "That request wasn't available" → "Den förfrågan var inte tillgänglig". `high`.
- **token (LLM usage unit): `token` / `tokens`** · kept identical to English in both CLDR branches (`sourceHash`
  `askCmdr.cost.tokens` carries `sameAsSourceJustification`). No native Swedish plural is attested in the reference pile
  for this (recent, AI-specific) sense of "token" (the pile's only hit is the older `säkerhetstoken` = security token, a
  different concept); Swedish tech press consistently keeps the bare English plural "tokens" for LLM usage. `tentative`
  (no reference-pile plural; convention from current Swedish tech usage).
- **usage / spending (AI cost tracking): `användning` / `utgifter`** · MS terminology (`usage` → `användning`,
  `spending` → `utgift`, pluralized for the settings section heading). `high`.
- **estimate, adverbial ("about {amount}"): `cirka`** · matches the existing sv catalog's own "cirka"/"ungefär" usage
  for approximate values (`indexing.scan.etaRough`, `onboarding.stepAi.local.help`). `high`.
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
  with) for the one-time opt-in heading, matching the screen's inviting tone; `askCmdr.consent.title` "Talk to Cmdr
  about your files" → "Prata med Cmdr om dina filer". `tentative` (stylistic choice, no single correct pile rendering
  for this warmer register).
- **importance (of a folder, the assistant's ranking feature): `vikt`; important → `viktig`** · no reference-pile hit
  (Cmdr-specific ranking feature); composed on the standard adjective/noun pair (`viktig`↔`vikt`), parallel to how
  `askCmdr.tool.importantFolders.*` already uses `viktig`. `tentative` (Cmdr-coined feature; review).
- **Cmdr repeated instead of a bare pronoun, when the sentence names Cmdr's own behavior**: per the established sv
  catalog convention (errors.json etc. always re-use "Cmdr" rather than "den"/"det"), `askCmdr.empty.hint` and
  `askCmdr.consent.noContents` repeat "Cmdr" across sentences rather than introducing an ambiguous pronoun. Where the
  antecedent is unambiguous within the same sentence (`settings.askCmdr.intro`'s "Ask Cmdr är skrivskyddad: den
  läser…"), a pronoun is fine.

## Network-drive image indexing pass (2026-07-13; `settings.mediaIndex.networkVolumes.*` + `settings.mediaIndex.alwaysIndex*` + `search.imageResults.networkOff`/`.paused`)

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
  (glossary) + `enhet` (drive); standard Swedish IT compound, matches how the drive surfaces to the user. `high`.
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

No `sameAsSourceJustification` needed: all 19 values differ from English.

## Quality pass: bulk rename, image-index scope, Ask Cmdr tool labels (2026-07-21)

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
  "{percentText} %", "Zoom återställd till 100 %."). `fileExplorer.imageIndex.indexingTooltip` had `{percent}%`; fixed
  to `{percent} %`. Note the contradicting `sameAsSourceJustification` on the out-of-scope key
  `indexing.progress.percentEta` ("this locale uses the same percent spacing and comma as English") — that justification
  is wrong for sv on both counts and is flagged for David. `high`.
- **"Ask Cmdr to prepare it again" → `Be Cmdr att förbereda den igen`** · the EN "Ask" is the sentence-initial
  imperative verb, not the feature name (the feature name would not be capitalized mid-sentence anywhere else in the
  string). Rendering it as "Be Ask Cmdr att…" stacked the verb on the product name. The user is inside the Ask Cmdr
  rail, so the referent is unambiguous. `high`.
- **photo → `bild`, uniformly** · re-confirms the network-drive pass's decision (Apple localizes the Photos app to
  "Bilder"). The four Ask Cmdr tool labels had drifted to `foton`; aligned to `bilder` so the whole photo-indexing
  surface ("Bildsökning", "Bilder indexerade", "Indexera bildinnehåll") reads as one feature. ⚠️ Four OUT-OF-SCOPE
  shipped keys still say `foton`/`Fotosökningen`: `askCmdr.consent.noContents`, `settings.mediaIndex.clip.description`,
  `settings.mediaIndex.clip.ready`, and `onboarding.stepOptional.mtp.desc` (that last one is fine as-is, it's about
  copying photos off a phone, not the search feature). They should be aligned in a follow-up. `high`.

No `sameAsSourceJustification` needed: all 54 values differ from English.

For the image-search index status badges (2026-07-22; the 11 `fileExplorer.imageIndex.*` badge/dot tooltips + 2
`settings.mediaIndex.showFileStatusIcons.*` keys). Small status indicators on image files, folders, and drives showing
image-search indexing state. Reuses the settled indexing family; new/confirmed terms:

- **image search (the feature): `bildsökning`** · already the catalog's own term (`settings.mediaIndex.card` =
  "Bildsökning"); definite `bildsökningen`. Compound `bildsökningsstatus` for the drive aria-label. `high`.
- **indexed (as a status on a `bild`): `indexerad` / `indexerade`** · en-word agreement with `bild` (glossary index
  family + shipped `settings.mediaIndex.networkVolumes.indexed` "{countText} bild indexerad / bilder indexerade"). The
  standalone file badge takes the en-word `Indexerad` (implied subject `bilden`, en-word), NOT Apple's neuter supine
  `Hämtat` pattern, because Cmdr's badge is always on an image. `high`.
- **waiting to be indexed: `Väntar på att indexeras`** · mirrors Apple Finder's badge AX pattern "Väntar på
  överföring/hämtning/uppdatering" (`macOS/Finder` AXBADGE4/5/6). Passive `indexeras` for the queued state. `high`.
- **re-index: `indexera om` (passive `indexeras om`)** · the `montera om`/`söka igenom på nytt` re-prefix pattern
  (glossary). "Changed since indexing; will be re-indexed" → "Ändrad sedan indexeringen; indexeras om" (`Ändrad` =
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
- **"is off" (a feature disabled for a drive): `är avstängd`** · en-word participle of `stänga av` (glossary
  enable/disable), agreeing with `bildsökning(en)`. `high`.
- **"still working" (indexing in progress, drive dot): `arbetar fortfarande`** · casual/friendly like the EN source;
  implied subject Cmdr. `high` (natural phrasing; no direct pile hit).
- Drive plural strings duplicate the invariant "på den här enheten är" inside both plural branches (the
  `progress.ofTotal` pattern) so the `indexerad`/`indexerade` adjective agrees in number without a second ICU block.

No `sameAsSourceJustification` needed: all 13 values differ from English.

## Image-indexing progress/settings UX pass (2026-07-23; `settings.mediaIndex.*` + `fileExplorer.imageIndex.file.indexing`)

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
  Apple-kisel" structure; the bare English term reads as a recognizable tech proper noun in Swedish. (If a native
  reviewer prefers Apple's Swedish marketing term, `Apple-kisel` is the apple.com/se rendering.) `tentative` (pile gap;
  kept per source instruction, flag for native review).
- **enable indexing / folders to index (card titles): `Aktivera indexering` / `Mappar att indexera`** · `aktivera`
  (enable) + `indexering`; `att`+infinitive for "to index". Sentence case. `high`.
- **delete model (a re-downloadable resource, reclaim disk): `Ta bort modell` (button) / `Tar bort…` (in progress) /
  `Ta bort modellen för semantisk sökning?` (confirm title)** · `ta bort` (remove-from-collection sense, NOT the
  destructive `radera`) since the model is re-downloadable; pairs with `Ladda ner modell` (`clip.download`) and its
  present-tense `Laddar ner…`. "reclaim {size}" → "frigör {size}" (verb of `reclaim.freed` "Frigjorde"). Confirm title
  reuses `Semantisk sökning`. `high`.
- **keyword / tag search (in the delete-confirm body): `nyckelordssökning` / `taggsökning`; combined
  `Nyckelords- och taggsökning`** · `nyckelord` (MS keyword) + `sökning`; `tagg` (catalog `Visa taggar`, "macOS
  Finder-taggar") + `sökning`. "keep working" → "fortsätter fungera". `high`.
- **Indexing now (badge tooltip + progress heading, same EN source/sourceHash 44501db): `Indexeras nu`** · passive
  present of `indexera` (implied subject `bilden`/the drive), meaning actively being processed now, distinct from the
  queued `Väntar på att indexeras` (`file.pending`). Serves both `fileExplorer.imageIndex.file.indexing` and
  `settings.mediaIndex.progressSummary.title`. `high`.

No `sameAsSourceJustification` needed: all 12 values differ from English.

## Delete-dialog trash switch + transfer From/To groups (2026-07-23; `fileOperations.delete.trashSwitch`/`confirmDelete` + `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`)

- **"Move to trash" (switch in the delete dialog, on = papperskorgen, off = permanent delete):
  `Flytta till papperskorgen`** · macOS Finder sv AL13/N153 verbatim; identical to this file's
  `transferDialog.titleVerbOnly` `other {Flytta till papperskorgen}` arm, so the switch and the confirm button read as
  one pair. `high`.
- **"Delete" (destructive confirm button while the switch is off): `Radera`** · settled delete verb, identical to
  `transferDialog.titleVerbOnly`'s `delete {Radera}` arm. `high`.
- **"From" / "To" (headings over the source path and over the destination volume + path): `Från` / `Till`** · Total
  Commander sv ships this exact label pair in its copy/move dialog (`662="Från: "`, `663="Till: "`); macOS "Flytta till"
  confirms `till` for a destination. The settled `mål` target noun stays for the destination CONTROLS (`Målvolym`,
  `Målsökväg`); the headings take the light prepositional pair the English uses. `high`.
