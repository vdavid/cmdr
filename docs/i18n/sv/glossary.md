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
- **word wrap: `automatiskt radbyte`** · MS terminology has the exact headword (term-id `134172`, SWE, noun); MS's
  "omslutning" is the generic `wrap` sense and the wrong fit. macOS has no "word wrap" of its own, so Tier 2 decides.
  See § Termdriftsgranskning. `high`.
- **branch (git): `gren`** · standard Swedish git term; MS's "förgrena" is the verb sense. `tentative`.
- **repository (git): `git-repository`** · keep the git loanword; MS's "centrallager" is the generic-storage sense.
  `tentative`.
- **startup disk: `startskiva`** · macOS Finder ("Startskiva", "Startskivevärde"). Boot drive. `high`.
- **Privacy & Security (macOS pane): `Integritet och säkerhet`** · macOS SystemSettings. `high`.
- **Full Disk Access (macOS permission): `Full skivtillgång`** · three live macOS bundles agree, including the very pane
  the user lands in: `Security.prefPane`, `SecurityPrivacyExtension.appex`, and `Sharing.appex` (`Localizable.loctable`,
  macOS 26.6.2 build 25G83, verified 2026-08-30). `high`. ❌ Not the descriptive `fullständig åtkomst till skivan`: the
  runtime already substitutes Apple's real label into `{full_disk_access}` (`system-strings.svelte.ts` hydrates it from
  the OS), so a paraphrase elsewhere left the app calling one setting two names. Capitalized in English ("Requires Full
  Disk Access") = the setting name, so `Full skivtillgång`; lowercase in running prose keeps the same words
  uncapitalized (`full skivtillgång`), which is what makes the setting findable. Agreement note: `tillgång` is an
  en-word, so `Full skivtillgång är ganska kraftfull`, never `kraftfullt`.
- **Local Network (macOS permission): `Lokalt nätverk`** · live macOS `SecurityPrivacyExtension.appex` (2026-08-30).
  `high`.
- **Privacy & Security (macOS pane): `Integritet och säkerhet`; Quick Look: `Överblick`** · both re-verified live on
  macOS 26.6.2 (2026-08-30). `high`.
- **Disk Utility > First Aid: `Skivverktyg > Skivkontroll`** · live `Disk Utility.app` `sv` (`First Aid` →
  `Skivkontroll`, `Disk Utility — First Aid` → `Skivverktyg – Skivkontroll`, 2026-08-30). `high`. ❌ Not `Skivhjälpen`,
  which names no menu item Apple ships.

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
- **rollback (undo a partial transfer): `ångra`** · macOS `sv` `Undo` → "Ångra", Nautilus `sv` "_Ångra Kopiera" · high.
  Not `återställ`: that IS `restore`, and rollback doesn't restore what it overwrote. Full arbitration over the 14 keys:
  § "Rollback-familjen" at the end of this file.
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
- **dismiss: `avfärda`** · toast/alert dismiss button, kept distinct from closing a dialog ("Stäng"). `high` — see §
  Progress chip + failure notice at the end of this file for the macOS AppKit evidence that settled it.
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
- **queue (the bare noun): `kö`; queued status `Väntar`** · Total Commander uses the bare noun "Kö" for its job queue;
  Thunar renders "Job queued" as "Jobb köade" (verb "köa"). The per-row queued state reads "Väntar" (waiting its turn).
  The toolbar "Queue" button (send-to-background) on the progress dialog is the bare noun "Kö". `high`. ⚠️ **The
  window's NAME is no longer `överföringskö`** — see § Operation queue (2026-08-08) at the end of this file; it is now
  `Åtgärdskö` / definite `åtgärdskön`. Don't reintroduce `överföringskö`.
- **background / send to background: `i bakgrunden` / `skicka till …kön`** · Total Commander ("…överföringar i
  bakgrunden", "i bakgrunden"). "Keep this running in the background" → "Håll igång den här i bakgrunden"; "Send to the
  operation queue" → "Skicka till åtgärdskön" (sending to the queue IS sending to the background here). `high`.
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
- **roll back / rollback (reverse a logged operation): SUPERSEDED, see § "Rollback-familjen" at the end of this file.**
  The status-chip reasoning below still holds; only the word family changed, to `ångra`. ~~reuse
  `återställ`/`återställa`/`återställer`/`återställd`~~ · the settled rollback family (glossary rollback entry +
  `fileOperations.transferProgress` "Återställer"/"Återställ"). Status chips: notRollbackable → "Går inte att
  återställa", rollbackable → "Går att återställa", rollingBack → "Återställer", rolledBack → "Återställd",
  partiallyRolledBack → "Delvis återställd". Command description "roll them back" → "återställ dem". `high`. NOTE:
  `settings.operationLog.intro` (already shipped) phrases the same concept as "ångra åtgärder"; the dialog uses the
  `återställ` family for consistency with the transfer-rollback surface — flagged for David if he wants the intro
  aligned.
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

## Drive-indexing master-switch pass (2026-07-27; the 5 `driveIndex.*IndexingOff` / `settings.indexing.masterOffNote`/`.overriddenBadge` keys)

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
  `Kräver X` is the catalog's settled pattern for it (`Kräver Apple Silicon`, `Kräver Fullständig åtkomst till skivan`,
  `Kräver en internetanslutning`) and stays badge-short (23 chars vs the English 21). The faithful-but-longer
  alternative if a native reviewer wants literal parity is `Av tillsammans med indexeringen`. `high` for the term,
  `tentative` for the state→requirement reframing (flagged for David).
- **"stays unindexed" → `indexeras inte`** · chose the attested negation (`Inte indexerad än`, Dolphin "inte indexerad")
  over coining `oindexerad`, which no source has. `high`.
- **"picks up where it left off" → `fortsätter där den slutade`** · natural Swedish; no pile hit for the idiom. `high`.

No `sameAsSourceJustification` needed: all five values differ from English.

## Enhetsindex: genomsökningen som letar ändringar (2026-07-28)

- **"Checking for changes" (run-kind header) → `Kontroll av ändringar`** · nominal phrase matching the sibling headers
  (`Första fullständiga genomsökningen`, `Snabb uppdatering`); `Kontrollerar` is macOS SV's checking verb (Finder BN9
  "Kontrollerar om innehållet…"), `ändringar` is catalog-settled (`senaste ändringarna`). Chose `kontroll` over the
  colloquial `koll` (which `tooltipCoalesced` uses only inside the idiom `tappade koll på`) · high.
- **"Update the file list" → `Uppdatera fillistan`** · composed from the settled siblings `Spara fillistan` +
  `Uppdatera index` · high.
- **"the check running right now" → `genomsökningen som pågår just nu`** · reuses `genomsökning` as this catalog's
  settled word for a full check (`tooltipCoalesced`: "Cmdrs nästa fullständiga genomsökning") and that string's closing
  `rättar till det` · high.

## Stalled-transfer notice (2026-07-31; the 7 `fileOperations.transferProgress.stall*`/`.close` keys + `queue.row.stalled`)

The copy/move dialog and the queue row when a transfer has stopped moving (a parked SMB share or phone). The notice
replaces the ETA line, so it must stay calm and never reach for `fel`/`misslyckades`.

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
  `Document.json`), verbatim. Distinct from the neighbouring `Avbryt` (Cancel), and it matches the glossary's dismiss
  entry, which reserved `Stäng` for closing a dialog and `avfärda` for dismissing a toast. `high`.
- **"The log has the details." → `Detaljerna finns i loggfilen.`** · same sentence shape the catalog already ships in
  `askCmdr.renameUndo.refusedBatches` ("Detaljerna finns i åtgärdsloggen."). Chose `loggfilen` over a bare `loggen`
  because Cmdr has TWO logs and `åtgärdsloggen` (the undo history) is the wrong one here; `loggfil` is the catalog's
  settled name for the disk log (`settings.logging.openLogFile` "Öppna loggfil"). `high`.
- **ICU shape:** the `stallInFlight` trailing clause is INSIDE both plural branches, unlike English, which leaves it
  outside. Swedish needs it there: the predicative adjective and participle agree with the counted noun
  (`öppen`/`skriven` singular vs `öppna`/`skrivna` plural), so a shared tail would be ungrammatical in one branch.
  Verified with `IntlMessageFormat(msg,'sv')`: 1 → `one`, 2 and 0 → `other`.
- No `sameAsSourceJustification` needed: all eight values differ from English.

## Kopierad sökväg: urklippsbekräftelsen (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

En nyckel: raden i informationsnotisen efter ⌘⌥C. Sökvägen visas under den, på egen rad med fast teckenbredd, så den är
INTE en platshållare i meningen: meningen slutar med kolon och måste fungera utan den.

- **"Copied the path, it's now on your clipboard:" → `Kopierade sökvägen, den finns nu i urklipp:`** · återanvänder
  `path → sökväg` och `clipboard → urklipp` ur glossaret (macOS Finder) · high. Preteritum först speglar systernotisen
  `Kopierade {countText} objekt`, och `i urklipp` (inte `på urklipp`) är den redan settlade prepositionen
  (`clipboard.empty` = "Inga filer i urklipp."). Inget possessivt "ditt urklipp": det finns bara ett.
- Inget `sameAsSourceJustification` behövs: värdet skiljer sig från engelskan.

## Operation queue: kön byter huvudord (2026-08-08; the 14 `queue.*` / `commands.queueShow.*` / `fileOperations.transferProgress.queue*` keys)

The English window was renamed from **"Transfer queue"** to **"Operation queue"**, because it lists deletes, trashes,
renames, folder and file creations, and archive edits too, not only copies and moves; "transfer" also already means
copy-or-move one level down in Cmdr (the transfer progress dialog, the transfer driver). So the source widened from a
narrow word to the CATEGORY word, and Swedish widens the same way. This is a meaning change, not a wording tweak.

- **operation (the category word for a copy, move, delete, trash, rename, create, or archive edit): `åtgärd`** (common
  gender: en åtgärd, definite `åtgärden`, plural `åtgärder`) · already this catalog's settled head noun
  (`operationLog.*` → `Åtgärdslogg`, `settings.section.operationLog`, `åtgärdshistorik`, the `Åtgärd:` field label), and
  macOS Finder sv confirms it in exactly Cmdr's sense: "Du kan inte byta namn på ”^0” eftersom **en annan åtgärd** pågår
  just nu, t.ex. flytt eller kopiering av ett objekt eller tömning av papperskorgen"
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
- **SUPERSEDED: `överföringskö` / `Överföringar`** (the transfer-queue pass above). Kept visible only so a future pass
  recognizes it as the old name and doesn't restore it. `överföring` itself is NOT retired: it stays the right word for
  a copy/move in flight (the transfer progress dialog, `Överföringen står stilla`, `Överföringsmetod`), which is exactly
  the narrower sense English kept one level down.

No `sameAsSourceJustification` needed: all 14 values differ from English.

## Progress chip + failure notice (2026-08-08; the nine `queue.row.dismiss*` / `queue.toolbar.dismissAll` / `queue.failureToast.*` / `queue.chip.*` keys)

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
  `·` before the count would fix trash and worsen the seven common arms). Flagged for David; the fix belongs in the
  English key's shape, not in one locale.

No `sameAsSourceJustification` needed: all nine values differ from English.

## Standalone conflict prompt (2026-08-09; `fileOperations.operationConflict.context`/`.pausedNote`)

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
  (matching this catalog's own "ett arkiv" in `errors`/`fileExplorer`/`operationLog`/`settings`). ⚠️ The
  Archive-password section above calls `arkiv` common-gender; that holds only for that dialog's `den` pronoun choice,
  not for the noun, which takes `ett`/`arkivet`. `high`.
- **"Everything else is paused until you answer." → `Allt annat är pausat tills du svarar.`** · `pausat` is the settled
  queue status word `Pausad` (macOS Finder "Pausad", "Kopiering av ”^0” har pausats") in NEUTER agreement, because the
  subject is `allt annat`; keeping the same root is what makes the note and the queue rows read as one state.
  `tills du svarar` over `tills du har svarat`: the English is a simple present, and the shorter form stays calm under a
  button row. `high` for `pausat`, `tentative` for the `tills du svarar` clause (no pile hit for the idiom; composed).

No `sameAsSourceJustification` needed: both values differ from English.

## Empty-queue state of the queue button (2026-08-09; `fileOperations.transferProgress.background` + `.backgroundAria`)

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
  `i bakgrunden` (§ transfer-queue pass) and matches `queueAria`'s imperative shape ("Skicka till åtgärdskön"). `high`.
- **WCAG 2.5.3 containment: the label `I bakgrunden` sits inside the aria as `i bakgrunden`**, matching
  case-insensitively (identical letters, only the sentence-case initial differs) — the same bar English keeps
  ("Background" ⊂ "…in the background"). Exact-case containment isn't reachable in natural Swedish here: the aria is a
  clause that starts with its verb (`Håll`), and Swedish capitalizes nothing mid-sentence. See style.md § Label in Name
  for the trap this avoids: the bare noun `Bakgrund` is NOT a substring of `i bakgrunden` (indefinite vs definite), so
  the noun choice would have broken containment as well as the part of speech.
- No `sameAsSourceJustification` needed: both values differ from English.

## Quit gate: dialogen som stoppar ⌘Q medan något pågår (2026-08-10; the seven `main.quit.*` keys)

The modal Cmdr raises when the user quits while a copy, move, delete, trash, or archive edit is still running: a
question title, a reassuring body, the list of running operations under a small heading, a live countdown, and the two
buttons. Reuses the settled `åtgärd` head noun (§ Operation queue), `Pågår` (`queue.row.status`), `objekt`,
`delvis skriven` (§ Stalled-transfer notice), and `Avsluta` (the quit verb). New/settled:

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
  written" (§ Stalled-transfer notice, `transferProgress.stallInFlight` "kan redan vara delvis skriven/skrivna"), so it
  reuses that wording rather than coining `halvskriven`; neuter agreement with `det`, since the definite
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

No `sameAsSourceJustification` needed: all seven values differ from English.

## Usage stats: "anonym" dropped, "ett slumpmässigt id" named (2026-08-12; `settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

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
- No `sameAsSourceJustification` needed: every value differs from English.

## Frågan som stoppar en kö-rad + återställningsdialogen (2026-08-13; `queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, the four `fileOperations.rollbackConfirm.*`, and the reworded `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

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
- **"Roll back" / "Roll this operation back?" → `Återställ` / `Återställa den här åtgärden?`** · the settled `återställ`
  rollback family (matches `transferProgress.conflictRollback`); the bare-infinitive question mirrors `main.quit.title`
  ("Avsluta medan en åtgärd pågår?") · high
- **"Stop" in the rollback tooltip → `Stoppa`** · macOS Finder "Stoppa" (`PE107`, `SD23`, "stoppa processen och behålla
  en delvis kopia"). ❌ Never `Avbryt` here: that IS the Cancel button, which KEEPS the finished files, and the tooltip
  exists to say rollback doesn't · high
- **"so far" → `hittills`** · standard Swedish, no direct pile hit; unambiguous · tentative (convention)
- **the files an operation overwrote → `filer som den har skrivit över`** · the settled `skriv över` (style.md).
  English's "replaced" is the overwrite sense here, so don't reach for `ersätta` · high
- `foregroundBusyToast` no longer claims an operation is in the way ("Något annat är öppet här"): the blocker can be any
  dialog. "bring this one up" → `ta sedan fram den här` (`åtgärd` is common gender, so `den`) · high
- No `sameAsSourceJustification` needed: all eight values differ from English.

## Rollback-familjen: `återställ` ersatt av `ångra` (2026-08-13, 14 nycklar i `fileOperations`, `operationLog`, `commands`)

Rättar den `tentative`-markerade rollback-posten ovan och löser den inkonsekvens den själv flaggade
(`settings.operationLog.intro` sa redan `ångra åtgärder` medan dialogen sa `återställ`).

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

## Kedjade namnbyten: toasten som räknar de övriga (2026-08-18; `fileExplorer.rename.chainKeptOriginalNameAndOthers`)

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
- Inget `sameAsSourceJustification` behövs: värdet skiljer sig från engelskan.
- ⚠️ Flaggat för en framtida granskare: `{reason}` är text Cmdr inte helt styr över och avslutas utan punkt, så meningen
  börjar med en inskjuten främmande sats. Det fungerar i svenskan lika bra som i engelskan, men om en `reason` någon
  gång slutar med `?` eller `!` blir `. ` efter den fel i båda språken; det är då engelskans nyckelform som ska ändras,
  inte den här översättningen.

## Obekräftade namnbyten och det oanvändbara namnet (2026-08-18; `fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

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
- Inget `sameAsSourceJustification` behövs: alla tre värdena skiljer sig från engelskan.

## Föreslagna åtgärder: rutan för det Ask Cmdr föreslår (`suggestedOps.*`, `commands.suggestedOpsShow.*`, 2026-08-19)

- ops (agentens föreslagna filåtgärder) → `åtgärder`; titeln blir `Föreslagna åtgärder` · följer husets "File
  operations" → `Filåtgärder` · high
- approve → `Godkänn` · standard; valt framför macOS `Ta emot`, som hör till att ta emot en AirDrop-fil och inte till
  att låta något köra · high
- reject → `Avböj` · macOS Finder, paret Ta emot/Avböj i AirDrop-rutan (Tier 1) · high
- "This can't be undone" → `Det här går inte att ångra` · macOS Finder ("Den här åtgärden går inte att ångra"),
  förkortat till en etikettrad · high
- "Ask Cmdr's reason" → `Ask Cmdrs skäl` · genitiv på varumärket, enligt regeln om att märkesnamn får böjas · high

## Duplicera: kommandot som kopierar i samma mapp (`commands.fileDuplicate.*`, 2026-08-19)

- **duplicate (kommandot som kopierar markeringen i dess egen mapp) → `Duplicera`** · macOS Finder `sv`, menyn "Arkiv >
  Duplicera" (`N154`), plus "Duplicera objekt" och "Duplicerar objekt där de befinner sig" (verifierat på macOS 26.6.1,
  `Finder.app/Contents/Resources/sv.lproj`, 2026-08-19) · `high`. Krockar inte med `Kopiera` (F5) eller `Flytta` (F6).
- **"Make a copy of the selected files in the same folder" → `Skapa en kopia av de markerade filerna i samma mapp`** ·
  imperativ, som systerbeskrivningarna ("Kopiera markerade filer…"); `markerade filer` är katalogens term för selected
  files, och "samma mapp" är den mapp filerna redan ligger i · `high`.

## Inbyggda menyer: menyrad, snabbmenyer, fönstertitlar (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, 2026-08-19)

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
- **busy (volym som används) → `(upptagen)`** · Microsofts terminologi · high.
- **Eject → `Mata ut`, Disconnect → `Koppla från`, Remove (ur en lista) → `Ta bort`** · macOS Finder · high. `Radera` är
  fortfarande reserverat för permanent radering, enligt `style.md`.
- **Avsiktligt identiska med engelskan** (med `sameAsSourceJustification`): `menu.view.zoom`, `menu.tag.orange`,
  `menu.view.askCmdr`.

## Aviseringen när Cmdr fastnade på systemanslutningen (`fileExplorer.network.osMountFallback.*`, 2026-08-21)

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
- **"Dismiss" (krysset) → `Avfärda`** · glossarets dismiss-post (macOS AppKit `Avfärda popover`) och systernyckeln
  `lowDiskSpace.toast.closeTooltip`, som redan säger `Avfärda` · `high`. `Stäng` är fortsatt reserverat för dialoger och
  fönster.
- **Utelämnat `network` i "native SMB network connection"** · `SMB-nätverksanslutning` blir ett tungt trippelkompositum
  utan att tillföra något: SMB ÄR nätverk, och aviseringen visas i nätverksvyn. `macOS inbyggda SMB-anslutning` säger
  samma sak och läses som svenska.

## Namnbyten och volymsvar som inte gick igenom (`errors.mutation.*`, `errors.volume.*`, 2026-08-23)

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
- **Get Info-fönstret: `Finders fönster Visa info`** · glossarets `Visa info` + macOS egen konstruktion ("Visar fönstret
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
  utrumform i § Archive-password dialog gäller `fileOperations.archivePassword.message`, där adjektivet kongruerar med
  `{name}` (filen), inte med ordet `arkiv`.
- **stöder inte / kunde inte slutföra: `den åtgärden`** · `errors.volume.notSupported` och `.ioError` säger bara "that"
  på engelska; svenskan behöver ett huvudord, och beskrivningen namnger den begärda åtgärden. `åtgärd` enligt glossarets
  operation-post och macOS ("Åtgärden kan inte slutföras eftersom den inte stöds") · `high`.
- **Ingen skuld på personen i namnbytesspärrarna** · "Renaming can't take an item out of an archive" blir
  `Ett namnbyte kan inte flytta ut ett objekt ur ett arkiv`, inte "Du kan inte …". `ur` för ut-ur-behållare enligt
  glossarets arkivpost ("ta bort … ur zip-arkivet") · `high`.
- **Citattecken runt `{path}`: `”…”`** · engelskan använder raka `"` här, men svenska katalogen har `”…”` genomgående (§
  Cross-file consistency reconciliation) · `high`. `{path}` är okontrollerad text, så meningarna slutar på den där det
  går (`Det finns inte längre något på ”{path}”.`).
- **Inga `sameAsSourceJustification`** · alla 31 värden skiljer sig från engelskan.

## Papperskorgen i namnbytesspärrarna (`errors.mutation.trashNotSupported`/`.trashRefused`, 2026-08-23)

Två nycklar till i `errors.mutation.*`-familjen (RAW, ingen ICU), samma enradsyta under namnfältet.

- **"has no Trash" → obestämd form `har ingen papperskorg`** · glossarets arkivpost säger redan "Det finns ingen
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
  (glossarets "keep working" → `fortsätter fungera`), inte om att processen levde vidare, vilket är hela poängen här.
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
- Inget `sameAsSourceJustification` behövs: båda värdena skiljer sig från engelskan.

## Inställningstexten för rapporter gäller nu båda utfallen (`settings.updates.crashReports.description`)

Reglaget skickar en rapport även när en panic i bakgrunden INTE tog ner appen, så hjälptexten kan inte längre handla
bara om att Cmdr avslutas. Allt är hämtat från kraschdialogsavsnittet ovan, i presens:

- **`när Cmdr avslutas oväntat`** från `crashReporter.dialog.body.ended`; **`stöter på ett problem i bakgrunden`** från
  `.keptRunning` · high. Presensformen är ren morfologi, inte ett nytt termval.
- **`en rapport`** utan `krasch`-ledet, eftersom meningen täcker båda utfallen · high. ❌ ETIKETTEN
  `settings.updates.crashReports.label` står kvar som `Skicka kraschrapporter`: det är inställningens namn.
- **Andra meningen hämtad från `crashReporter.dialog.privacyNote`** (`vilken del av koden som stötte på problemet`), i
  stället för `kraschplats`, som bara stämde vid en krasch · high.

## Utmatning och frånkoppling som inte gick igenom (`errors.eject.*`, 2026-08-23)

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
- **Inga `sameAsSourceJustification`** · alla nio värden skiljer sig från engelskan.

## Papperskorgs-toasten: ångra och gå till papperskorgen (2026-08-27; `fileOperations.trash.*` + `commands.fileGoToTrash.*`)

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
- Inga `sameAsSourceJustification` behövs: alla nio värden skiljer sig från engelskan.

## Komplettera en redan skickad felrapport (2026-08-28; `errorReporter.amend.*` + `.amendedToast.message` + `.autoSentToast.viewOrAddNotes`)

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
- **Inga `sameAsSourceJustification`** · alla elva värden skiljer sig från engelskan.

## Markeringsdialogen: markera och avmarkera filer (`selection.*`, 2026-08-29)

- **select (filer via ett mönster) → `markera`; deselect → `avmarkera`** · macOS Finder `sv`, `MenuBar.json` `172.title`
  = ”Markera allt” och `300488.title` = ”Avmarkera allt” (Tier 1, kontrollerat mot det körande systemet, macOS 26.6.2,
  build 25G83, 2026-08-29). Microsoft Terminology säger detsamma (`deselect` → ”avmarkera”, term-id `44738`, SWE), och
  Total Commander `sv` (`WCMD.INC.utf8` rad 239–248: ”Markera alla filer”, ”Avmarkera grupp: enbart filer”) håller samma
  par i den ortodoxa tvåpanelsvärlden · `high`. Svenskan har alltså ett riktigt transitivt verb för båda hållen, till
  skillnad från tyskan och nederländskan, så hela familjen (`menu.select.*`,
  `commands.selectionSelectFiles`/`…DeselectFiles`, `selection.dialog.title.*`, `selection.action.*`) använder samma
  ordpar rakt igenom.
- **`Markera` är att markera objekt, `Välj` är att välja ett alternativ.** Redan satt i § Select-menyn ovan; det är
  därför dialogtiteln heter `Markera filer` och inte `Välj filer`.
- **recent selections → `senaste markeringar`** · speglat på syskonen i `queryUi.recent.*` (”senaste sökningar”), samma
  grammatik, bara `sökningar` → `markeringar`. `markering` är den satta termen för selection (§ Terms) · `high`.
- **`selection.recent.applyAria` följer `search.recent.runAria`** · där står ”Kör senaste {mode}-sökning: {query}”, här
  ”Använd senaste {mode}-markering: {query}”. `apply` → `Använd` från macOS AppKit (`NSFontOptionsPanel` `100411.title`
  och `NSPreferences` `7TY-1Z-cs2.title` = ”Använd”) · `high`. `{query}` ligger sist efter kolon, så vilken användartext
  som helst får plats.
- **Enter-tangenten heter `Retur` på svenska** · `search.runHint` säger redan ”Tryck på Retur för att söka”, alltså
  ”Tryck på Retur för att filtrera” · `high`. ⚠️ `queryUi.recent.popoverHint` säger fortfarande `Enter` i sin
  `<selectKey>`-tagg; det är en avvikelse värd en egen städrunda, inte något den här passen ändrade.
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
- Inga `sameAsSourceJustification` · alla femton värden skiljer sig från engelskan.

## Termdriftsgranskning: en sak, ett namn (2026-08-30)

Hela katalogen gicks igenom med `i18n-check-term-consistency` plus de tre manuella passen i
`docs/guides/i18n-translation.md` § "Auditing a finished locale for term drift". macOS-belägg är kontrollerade mot det
körande systemet (macOS 26.6.2, build 25G83, 2026-08-30) via `Finder.app`/`Safari.app` per-nib `.strings` och
`.loctable`, med `en_GB.lproj` som engelsk sida; pilen (`_ignored/i18n/sv/`) står för Microsofts terminologi och Tier 3.

### Fixat: samma engelska sa två saker på svenska

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
  den `tentative`-markerade `radbrytning` i § Terms. macOS har ingen egen "word wrap": TextEdit `sv` säger
  `Anpassa texten till fönstret` om en annan funktion (Wrap to Window) och `Radbrytning` bara inne i utskriftspanelens
  `Radbrytning efter sidans storlek`. Tier 1 saknar termen, alltså avgör Tier 2.
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

### Fixat i den manuella passningen (engelskan skiljer sig, så checken ser det inte)

- **Hide (imperativ) → `Göm`; hidden (adjektiv) → `dold`** · `high`. macOS Finder `sv` säger `Göm X` i elva levande
  strängar (`Göm sidofältet`, `Göm verktygsfältet`, `Göm förhandsvisning`, `Göm tillägg`, `Göm statusfältet`, …) och
  noll `Dölj`; pilen ger fjorton till, inklusive naket `"Hide": "Göm"`. Microsoft (`dölja`, term-id `61374`) och KDE
  Dolphin (`Dölj filterrad`) säger tvärtom, alltså en macOS-mot-Windows-delning där macOS vinner. Adjektivet stannar
  däremot på `dold`: Nautilus ("Om dolda filer ska visas"), Thunar ("Sortera dolda filer efter andra filer") och Total
  Commander (`5154="Visa &dolda filer …"`) är eniga, och `dolda filer` är den etablerade svenska filsystemtermen. ⚠️
  `Suppress` är ett annat engelskt verb och behåller `Dölj` (`settings.fileViewer.suppressBinaryWarning.label`,
  `settings.fileExplorer.suppressQuickLookHint.label`). Bonus: `commands.viewShowHidden.label` slipper stamupprepningen
  `dölj dolda` och heter nu `Visa eller göm dolda filer`.
- **download → `hämta` / `hämtning`** · glossarets satta term (macOS `Hämtade filer`) · `high`.
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

### Gränser: båda formerna är rätt, platta inte ut dem

Var och en av de tio första raderna motsvarar en post i `i18n-term-consistency-allowlist.json`; den engelska
källsträngen står i parentes.

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
- **`Error` → `Fel` bara i diagnostikkontext, annars `Problem`** (`"Error"`) · `high`. `settings.updates.errorPrefix` är
  en diagnostikrad och engelskans `@key` säger rakt ut att ordet är okej där;
  `fileExplorer.network.browser.status.error` står bland `Kan inte nås`, `Tidsgränsen nåddes` och
  `Inloggningen gick inte`, där `style.md` förbjuder etiketten `fel`.
- **`(unknown)` böjs efter det underförstådda huvudordet** (`"(unknown)"`) · `high`.
  `fileExplorer.network.browser.unknown` ersätter ett `antal` (neutrum) → `(okänt)`;
  `fileOperations.transferProgress.sizeUnknown` ersätter en `storlek` (utrum) → `(okänd)`. Katalogen gör redan samma sak
  utanför checkens synfält: `ai.cloud.unknownError` = `Okänt fel`, `ai.local.modelUnknown` = `Okänd`,
  `askCmdr.cost.unknown` = `kostnad okänd`.
- **`Modified` → `Ändrad` som attribut, `Ändrade` som filterpastill** (`"Modified"`) · `high`.
  `fileExplorer.columns.modified` och syskonen beskriver EN fils datum; `shortcuts.section.filterModified` står bredvid
  `Alla` och `Konflikter` och filtrerar en mängd kommandon, så pluralen kongruerar med mängden. Radmärket intill heter
  fortfarande `Ändrad från förval`, singular, för att det gäller en rad.
- **`Put back …` → `Lade tillbaka …` ur papperskorgen, `De gamla namnen återställdes …` för namn**
  (`fileOperations.trash.undone` respektive `askCmdr.renameUndo.undone`/`.partial`) · `high`. Redan satt i §
  Papperskorgs-toasten: `lägga tillbaka` är Finders `Put Back` (`N153.1`), och `återställa` är reserverat för
  `askCmdr.renameUndo.*`, där de gamla NAMNEN kommer tillbaka och ingenting flyttas. Engelskan delade en gång en enda
  sträng mellan de två handlingarna och har nu skilt dem åt; svenskan höll dem isär hela tiden. Exakt lydelse: § Shared
  `en` fixes (2026-08-30) sist i filen.
- **`File` → `Arkiv` som menyradsrubrik, `Fil` överallt annars** (`"File"`) · `high`. Redan satt i § Inbyggda menyer
  (Finder `300764.title`/`83.title`, AppKit `MenuCommands`). `suggestedOps.columnFile` är en kolumnrubrik, inte en meny.
- **`View` → `Innehåll` som menyradsrubrik, `Visa` som åtgärd** (`"View"`) · `high`. Redan satt i § Inbyggda menyer
  (Finder `206.title`/`207.title` OCH Safari `200.title`). `menu.file.view`, `commands.fileView.label` och
  funktionstangentraden är F3-åtgärden.
- **`Select` → `Markera` för objekt, `Välj` för ett alternativ** (`"Select"`) · `high`. Redan satt i § Select-menyn och
  § Markeringsdialogen; `ui.select.placeholder` är en rullgardins platshållare.
- **`Zoom` → `Zoom` (substantiv, textzoom-undermenyn) mot `Zooma` (verb, Fönster-menyn)** (`"Zoom"`) · `high`. Redan
  satt i § Inbyggda menyer (Finder `300667.title`).
- **`share` → `delad mapp` fritt stående, `-resurs` bara inne i en sammansättning, `Delningar` bara i värdlistans
  kolumn** · `high`. Svenskan kan inte sammansätta en tvåordsfras, så Microsofts `-resurs` står kvar där en
  sammansättning krävs: `settings.section.smbNetworkShares` (`SMB-/nätverksresurser`),
  `settings.appearance.tintSmb.description`, `settings.network.directSmbConnection.*`,
  `settings.network.timeoutMode.description`, `settings.advanced.mountTimeout.description`,
  `settings.summary.smbNetworkShares` (`resurscache`), `settings.indexing.askForEachDrive.description`. Kolumnrubriken
  `fileExplorer.network.browser.colShares` (`Delningar`) och dess räknare `fileExplorer.network.share.shareCount`
  behåller kortformen av breddskäl, precis som § Terms redan tillåter. Allt annat är `delad mapp` / `delade mappar`.

### Konvention: ett naket engelskt `All` blir `allt`, inte `alla`

macOS `sv` säger `Markera allt` och `Avmarkera allt`, aldrig `alla`. Utan utsatt huvudord dinglar `alla` (alla vad?),
medan neutrumformen `allt` står för sig själv. Gäller `Select all`, `Deselect all` och `Reset all to defaults`. Med
huvudord böjs det förstås normalt (`Stäng övriga flikar`, `Ångra alla omgångar`). Också noterad i `style.md`.

### Apples egna namn: kontrollera dem mot det körande systemet, inte mot minnet

Den dyraste klassen av drift i det här passet var inte två svenska ord för en engelsk term, utan ett svenskt ord för
något Apple redan har döpt. Copy som PEKAR på en systemyta måste stava ytan som macOS stavar den, annars skickas
användaren att leta efter ett menyalternativ som inte finns. Tre rättade, alla verifierade live på macOS 26.6.2 (build
`25G83`, 2026-08-30):

- **`Full Disk Access` → `Full skivtillgång`** i 19 nycklar (`onboarding.stepFda.*`, `onboarding.stepAi.banner*`,
  `search.coverage.*`, `downloads.fda.message`, `common.downloadsFdaHint`, `askCmdr.wake.needsFullDiskAccess`,
  `settings.behavior.…globalGoToLatestShortcut.enabled.description`). Se § Terms för belägget och kongruensen. Det som
  gjorde felet osynligt: `{full_disk_access}` i `errors.*` hämtas från OS:et i körningen, så appen visade Apples namn på
  ett ställe och en egen omskrivning på nitton andra.
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

## Shared `en` fixes: menu wording, System Settings tokens, name-restore verb (2026-08-30)

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

## En halvt ångrad åtgärd: att ångra klart (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`, 2026-08-30)

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
- Inga `sameAsSourceJustification` · alla fem värden skiljer sig från engelskan, och inget värde innehåller en apostrof,
  så ICU:s dubblering `''` blir aldrig aktuell. `{folder}` står oförändrad.

## Toasten efter en ångrad kopia eller flytt (2026-08-31; de 18 `fileOperations.cancelRollback.*` + omskrivna `rollbackConfirm.body`)

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
  (style.md § delete), och alla tre `rollbackConfirm.bodyUndo*` säger redan `raderar` om exakt den här handlingen ·
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
- Inga `sameAsSourceJustification` · alla 18 värden skiljer sig från engelskan, och inget värde innehåller en apostrof,
  så ICU:s dubblering `''` blir aldrig aktuell.
