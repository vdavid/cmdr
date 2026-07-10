# sv glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/sv/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: [i18n-translation.md](../../guides/i18n-translation.md).

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

- **Action (field label before the Copy/Move · Trash/Delete segmented control): `Åtgärd:`** · macOS Finder ("Åtgärd",
  standalone label) and MS terminology both render action → "åtgärd"; matches the glossary's `åtgärden {verb}` framing.
  Keep the trailing colon. `high`.
- **Route (field label before the `source → destination` line in the copy/move dialog): `Rutt:`** · Cmdr-coined label
  with no direct source. EN deliberately chose "Route" over "Path" to convey the from→to of a transfer, so kept distinct
  from `sökväg` (= path; MS renders the filesystem-route sense of "route" as "sökväg", but that collides with the path
  term). "Rutt" is the cognate that reads as a from-A-to-B route and stays distinct. Keep the trailing colon.
  `tentative` (Cmdr-coined; review).
- **Scanning… (tooltip + SR label on the counting spinner): `Söker igenom…`** · matches this file's
  `transferProgress.stageScanning` ("Söker igenom") and the glossary `genomsökning` / `söker igenom` scan-pass entries.
  Unicode ellipsis mirrors the EN source per the ellipsis rule. `high`.
- **Scan complete (tooltip + SR label on the done checkmark): `Genomsökning klar`** · scan noun `genomsökning`
  (glossary) + macOS "Klar" (done/complete, 12 pile hits). Pairs with `Söker igenom…`; mirrors EN's gerund→noun form
  shift. `high`.
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
  log). Applied to `operationLog.dialog.title` and `commands.logOperationLog.label` so the command, the settings section,
  and the dialog title all read the same word. `high`.
- **operation (a logged file operation): `åtgärd`** (definite `åtgärden`, plural `åtgärder`) · matches
  `settings.operationLog.*` ("loggade åtgärder", "gå igenom din historik") and the `åtgärden {verb}` framing. `high`.
- **history (operation history): `historik`; "operation history" → `åtgärdshistorik`** · `settings.operationLog` uses
  "historik"/"Behåll historik i"; compounded åtgärd+historik for `loadError`. `high`.
- **roll back / rollback (reverse a logged operation): reuse `återställ`/`återställa`/`återställer`/`återställd`** · the
  settled rollback family (glossary rollback entry + `fileOperations.transferProgress` "Återställer"/"Återställ"). Status
  chips: notRollbackable → "Går inte att återställa", rollbackable → "Går att återställa", rollingBack → "Återställer",
  rolledBack → "Återställd", partiallyRolledBack → "Delvis återställd". Command description "roll them back" → "återställ
  dem". `high`. NOTE: `settings.operationLog.intro` (already shipped) phrases the same concept as "ångra åtgärder"; the
  dialog uses the `återställ` family for consistency with the transfer-rollback surface — flagged for David if he wants
  the intro aligned.
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
