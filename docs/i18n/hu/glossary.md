# hu glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/hu/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

Core UI terms (pane, tab, volume, drive, folder, file, move, copy, rename, delete, trash, cancel = `Mégsem`, eject,
disconnect, share, search, sort, settings, index, overwrite, server = `szerver`) are sourced and fixed in `style.md` §
Terminology and glossary; use those verbatim. Below are the terms settled while translating `fileExplorer.json` (first
pass, 2026-06-21).

- host: `gép` (column `Gépnév` = hostname) · mac (network-browser nib: "Szervercím", "Csatlakozás"), ms · high. A
  network host in the SMB browser. macOS calls the manual-connect entity `szerver`; an auto-discovered box is a `gép`.
- mount: `csatolás` · mac ("csatol", "felcsatolni", "nem csatolható") · high. Verb `csatol`, noun `csatolás`.
- read-only: `csak olvasható` · mac, ms · high.
- guest: `vendég` · mac ("Vendég"), ms · high.
- sign in / log in: `bejelentkezés` · mac, ms · high. Credentials = `hitelesítő adatok`; authentication = `hitelesítés`.
- refresh: `frissítés` · mac ("Frissítés"), ms · high.
- retry: `újrapróbálkozás` / button `Újra` · ms · high. Short button stays `Újra`; progress text `Újrapróbálkozás…`.
- timeout: `időtúllépés` · ms, mac · high.
- home folder: `saját mappa` · mac ("Saját mappa") · high.
- favorite: `kedvenc` · mac ("Kedvenc"), ms · high. Cmdr's named-favorite feature, not a generic bookmark.
- broken symlink: `törött szimbolikus link` · ms (symbolic link = "szimbolikus hivatkozás/link"), descriptive ·
  tentative. macOS surfaces alias/`hivatkozás`; for the file-system symlink the technical `szimbolikus link` reads
  clearer.
- column header `Ext` (extension): `Kit` · abbreviation of `kiterjesztés`, matching the tight 3-letter English `Ext` ·
  tentative. No Tier-1 abbreviation source; mirrors the English column's terseness.
- error (status fallback, status cell): rendered as `Probléma` (not `Hiba`) · style-guide voice rule (no bare "hiba"
  label) · high. Applies to `status.error`, `tooltip.errorWithType`.
- scan (index): `átvizsgálás` · descriptive (ms "vizsgálat") · tentative. "Rescan now" = `Újbóli átvizsgálás`.
- Keychain → `kulcskarika` · macOS Hungarian · high. The localized Apple feature name (Decision 1: localize what Apple
  localizes, like Quick Look — NOT a verbatim brand). Apple's Hungarian Mac User Guide uses `kulcskarika` for the store
  (e.g. `iCloud-kulcskarika`) and `Kulcskarika-hozzáférés` for the Keychain Access app (verified on support.apple.com
  hu-hu Mac User Guide + macmag.hu, web, 2026-06-21). The store sense declines case-by-case (`a kulcskarikában`,
  `a kulcskarikából`, `a macOS kulcskarika`); the app name is `Kulcskarika-hozzáférés`. System keyring (generic) =
  `kulcstartó`.

UI section names captured (volume-switcher group headings, for cross-file consistency): Favorites = `Kedvencek`, Volumes
= `Kötetek`, Cloud = `Felhő`, Mobile = `Mobil`, Network = `Hálózat`. Settings location referenced in copy:
`Beállítások > Billentyűparancsok` (Settings > Keyboard shortcuts).

Settled while translating `errors.json` (second pass, 2026-06-21):

- host (remote machine in error prose): `gazdagép` · ms · high. NOTE the split with the prior `host: gép` entry above:
  that one names the SMB-browser hostname column (`Gépnév`, an auto-discovered box); in connection-failure error prose
  ("the host is down/unreachable") `gazdagép` is the natural full word. Different surface, different register.
- couldn't / failed (in body copy): `nem sikerült …` · mac ("nem sikerült megnyitni") · high. The style-guide-endorsed
  way to render "couldn't/failed" without a bare "hiba"/"sikertelen" label (e.g. "Nem sikerült beolvasni ezt a mappát").
- problem / glitch (soft "error" in explanations): `gond` · descriptive, extends the prior `error → Probléma` voice rule
  · tentative. Keeps error copy calm; "temporary glitch" → "átmeneti hiccup" (loanword kept, as it reads casual-native).
- retry (full-sentence imperative): `próbáld újra` / `lépj ide újra` (informal `te`, per Formality) · high. The short
  button stays `Újra` (prior pass); this is the in-sentence verb form, not the button label.
- permission (access right): `engedély` · mac, ms · high. The FDA/privacy GRANT sense uses `jogosultság`
  ("**{full_disk_access}** jogosultság").
- handle (open file handle): `leíró` ("nyitott leíró") · descriptive · tentative.
- git terms kept verbatim per the en `@key` do-not-translate notes: `git`, `worktree`, `commit`, `blob`, `repó` (repo).
  "working tree" = `munkafa`; "bare repo" = `csupasz repó`; "git browser" (Cmdr feature) = `git böngésző`.
- cloud mount (`cmVolumes.displayName`, descriptive not a brand): `Felhőcsatolás` · descriptive · tentative.
- your cloud provider (`genericCloudStorage.displayName`): `a felhőszolgáltatód` · descriptive · high.

### `{verb}`/`{Verb}`/`{gerund}` placeholder handling (errors.write.\*) — FLAGGED

These RAW tokens are filled with **English** words at runtime ("copy", "moving", "Copy"): `transfer-error-messages.ts`'s
`operationVerbMap` is hardcoded English, not localized. A raw English verb can't take a Hungarian case suffix, so —
following the fr pattern — each is wrapped in an apposition noun: `a(z) {verb} művelet` ("the {verb} operation"),
`a(z) {gerund} művelet közben` ("during the {gerund} operation"), `A(z) {Verb} művelet …` for titles. The `a(z)` covers
the unknown article of the inserted English word. **Flagged for review:** the operation verb stays English on screen
until that map is localized; the surrounding sentence is correct Hungarian regardless.

Settled while translating `settings.json` (2026-06-21):

### Settings section names (keep these verbatim wherever other files reference a Settings section)

Appearance = `Megjelenés` (mac), Colors and formats = `Színek és formátumok`, Zoom and density = `Nagyítás és sűrűség`,
File and folder sizes = `Fájl- és mappaméretek`, Listing = `Fájllista` (matches the `listing`/`fájllista` term),
Behavior = `Viselkedés`, File operations = `Fájlműveletek` (ms), File system watching = `Fájlrendszer figyelése`, Search
= `Keresés` (mac), AI = `AI` (kept), File systems = `Fájlrendszerek`, SMB/Network shares = `SMB-/hálózati megosztások`,
MTP = `MTP (Android/Kindle/kamerák)`, Git = `Git`, Viewer = `Megjelenítő` (matches the `viewer`/`megjelenítő` term),
Developer = `Fejlesztői`, MCP server = `MCP-szerver`, Logging = `Naplózás`, Updates & privacy =
`Frissítések és adatvédelem`, Advanced = `Speciális` (mac/ms), Keyboard shortcuts = `Billentyűparancsok` (mac), License
= `Licenc`.

### New terms

- System Settings (macOS app) → `Rendszerbeállítások` · mac · high.
- Privacy & Security (macOS pane) → `Adatvédelem és biztonság` · mac · high.
- Help (menu) → `Súgó` · mac · high.
- Downloads (folder) → `Letöltések` · mac · high.
- Full Disk Access → `Teljes lemezhozzáférés` · standard macOS Hungarian wording, no direct pile hit · tentative.
  FLAGGED.
- Local Network (permission) → `Helyi hálózat` · standard macOS wording, no direct pile hit · tentative. Mostly an
  OS-injected `{localNetwork}` placeholder; plain-text uses follow this. FLAGGED.
- toast (transient notification) → `buborék` · descriptive, no pile term · tentative. FLAGGED.
- chip / repository chip → `címke` (`repozitóriumcímke`) · descriptive, no pile term · tentative. FLAGGED.
- dirty state (git) → `piszkos állapot` · literal · tentative. FLAGGED.
- debounce → `pergésmentesítés` · descriptive, advanced-section only · tentative. FLAGGED.
- Wilting (date-color option) → `Hervadás` · descriptive (plant-wilting metaphor) · high.
- Smart (size option) → `Okos` · descriptive · high.
- "Reset to default" / reset → `Visszaállítás (alapértékre)` · ms ("Visszaállítás") · high.
- token (AI) → `token` · kept (standard AI term) · high; context window → `Kontextusablak`.

Brand/units kept verbatim (so legitimately identical-to-English): App, Lime, Port (label), AI, Git, ISO 8601, kB, MB,
GB, the `{placeholder}`-only path strings (`{systemSettings} > {appearance}`, the permission path), you@example.com.

Settled while translating `queryUi.json` + `commands.json` (2026-06-21):

- clipboard: `vágólap` · mac ("Vágólap") · high. copy/cut/paste = `Másolás` / `Kivágás` / `Beillesztés` · mac
  (MenuCommands) · high. select all = `Összes kijelölése`, deselect all = `Kijelölés megszüntetése` · mac · high.
- Get info (macOS) → `Infó megjelenítése` · mac (Finder N165) · high. The macOS Finder menu wording; reused per the en
  `@key` note ("reuse the localized macOS wording").
- Quick Look (macOS feature) → `Gyorsnézet` · macOS Hungarian (Finder TL14/N169.\*, AppKit) · high. The localized Apple
  feature name: Apple ships "Quick Look" as `Gyorsnézet` in Hungarian Finder, so Cmdr uses the term the user sees in
  their OS, never the English "Quick Look", in BOTH the menu-action label and user-facing prose. Inflects like Finder
  (accusative `Gyorsnézetet`, e.g. "a ⇧Space-szel a Gyorsnézetet"). Preview (non-mac) = `Előnézet`.
- Show in Finder (macOS) → `Megjelenítés a Finderben` · mac ("…megjelenítése a Finderben") · high. Finder kept verbatim.
- context menu → `helyi menü` · ms · high. "Open context menu" = `Helyi menü megnyitása`.
- zoom in / out → `Nagyítás` / `Kicsinyítés` (noun labels); verbs `nagyít` / `kicsinyít` · ms · high. "Zoom to 100%" =
  `Nagyítás 100%-ra`; zoom level = `nagyítási szint`.
- ascending / descending (sort order) → `növekvő` / `csökkenő` · gn/dolphin ("növekvő", "Csökkenő") · high.
- hidden files → `rejtett fájlok` · gn, dolphin · high. "Toggle hidden files" = `Rejtett fájlok ki-be`.
- wildcard → `helyettesítő karakter` · ms · high. glob/regex pattern row labels: glob → `Glob` (kept, no common HU
  equivalent, per en note), regex → `Regex` (kept).
- tab actions: new tab = `Új lap`, close tab = `Lap bezárása`, reopen = `Lap újranyitása`, pin = `Lap rögzítése` ·
  mirrors `lap` term + dc/dolphin patterns · high.
- New folder = `Új mappa`, New file = `Új fájl` · mac/gn · high.
- wizard → `varázsló` · ms · high. Onboarding (Cmdr's first-launch guide) → `Bevezető` (the command label `Bevezető…`;
  the wizard = `bevezető varázsló`) · descriptive, ms onboarding = `előkészítés` was a worse UI fit · tentative.
  FLAGGED.
- feedback → `visszajelzés` · ms · high.
- command palette (Cmdr UI) → `parancspaletta` · descriptive (parancs + paletta), no Tier-1 term · tentative. FLAGGED.
- "Coming soon" → `Hamarosan` · descriptive, common HU UI usage · high.
- "Make available offline" → `Elérhetővé tétel offline` · descriptive · tentative. "Remove download" =
  `Letöltés eltávolítása`.
- "{count}m/h/d/w/mo/y ago" relative-time chips: kept the terse English-style suffix letters (`{count} p`, `ó`, `n`,
  `h`, `hó`, `é`) as short HU abbreviations (perc, óra, nap, hét, hónap, év) + `ezelőtt`. "just now" = `most`.
- Page up / down → `Lapozás felfelé` / `Lapozás lefelé` · descriptive (no pile hit) · tentative.
- Brand/AI/units kept verbatim (legitimately identical-to-English in these two files): AI, Regex, Glob, Alt (modifier
  key name in aria labels), `⌘N`/`⌘H`/`⌘Enter`/`↑↓`/`Enter` glyphs, Cmdr, Finder, Total Commander, `~/Downloads`,
  `View > Zoom > 100%` (literal menu path), `100%`/`75%`/`125%`/`150%` zoom values, `*`/`?`/`!`/`>`/`<` filter glyphs.

Settled while translating `onboarding.json` + `fileOperations.json` (2026-06-21):

- merge (folders) → `egyesítés` · mac (Finder PE109 button = "Egyesítés", PE106 = "egyesítési funkció") · high.
- overwrite/replace (conflict): button verb `Felülírás` (glossary `overwrite → felülírás`); the macOS conflict button is
  `Csere`/`Lecseréli` (PE1/PE108), but Cmdr's UI says "Overwrite" not "Replace", so `Felülírás` is the faithful match ·
  high.
- skip (conflict policy) → `Átugrás` · mac (Finder PE112/AL6 = "Átugrás") · high. "Skip all" = `Összes átugrása`.
- rollback (undo partial transfer work / delete partials) → `visszagörgetés` · descriptive, no Tier-1 hit (macOS undo =
  `Visszavonás`, a different sense — full undo, not partial-cleanup) · tentative. FLAGGED. Distinct from cancel/Mégsem.
- destination → `cél-` prefix compounds · mac (Finder "célkötet", "céllemez") · high. destination volume = `Célkötet`,
  destination path = `Célútvonal`, target folder = `célmappa`.
- conflict (name clash) → `ütközés` · descriptive (ms "ütközés") · high. "Checking for conflicts" = "Ütközések
  keresése".
- cancel (a running transfer) → `megszakítás` · ms · high. NOTE the split: the button `Cancel` (closing a dialog) stays
  `Mégsem` (mac, glossary); `megszakít, megszakítás` is the verb/noun for stopping an in-progress operation.
- symlink target → `cél` · descriptive · high. (symlink itself = `szimbolikus link`, per `errors.json` pass.)
- hardlinked → `hardlinkelt` · loanword (no clean HU term; "merev hivatkozás" is rare/awkward) · tentative. FLAGGED.
- flush / "Writing the last piece" → `Az utolsó darab kiírása` · descriptive · tentative.
- Close (button) → `Bezárás` · mac (FR26) · high. Done = `Kész` (PW23). Dismiss = `Elvetés` · descriptive · tentative.
- Technical details → `Technikai részletek` · descriptive · high.
- macOS folder names (already localized by OS): Downloads = `Letöltések`, Documents = `Dokumentumok`, Desktop =
  `Asztal`, Applications = `Programok` · mac · high. macOS button "Quit & Reopen" = `Kilépés és újranyitás`, "Open
  System Settings" rendered as `{systemSettings} megnyitása`.
- agent (AI assistant) → `ágens` · descriptive · high. provider (cloud AI) → `szolgáltató`; API key → `API-kulcs`;
  endpoint → `végpont` · ms · high.
- "onboarding" surfaced as a noun ("Onboarding progress", screen-reader title) → `bevezető` (consistent with the prior
  `Onboarding → Bevezető` decision). Screen-reader title "Cmdr onboarding" = `Cmdr bevezető`.
- Legitimately identical-to-English residuals: `fileOperations.button.ok` = "OK" (universal),
  `fileOperations.shared.byteRate` = `<size></size>/s` (pure tag + unit marker, nothing to translate).

Settled while translating `licensing.json`, `ai.json`, `viewer.json` (2026-06-21):

- license → `licenc` · mac/ms · high. license key = `licenckulcs`, license type = `Licenc típusa`, validity =
  `Érvényesség`, activate = `aktiválás` (ms "aktivál"), renew = `megújítás` (ms).
- commercial (license tier) → `kereskedelmi` · ms · high. perpetual → `végleges` · ms · high. subscription =
  `előfizetés` · ms · high. Personal (tier) = `Személyes`, with `(ingyenes)` parenthetical kept.
- organization → `szervezet` · ms · high. Date-status lines avoid suffixing the locale-formatted `{date}`: "Érvényes
  eddig: {date}", "Lejárt ekkor: {date}", "Frissítések eddig: {date}" (postposition-style, dodges vowel-harmony on an
  unknown date string).
- file manager (tagline) → `fájlkezelő` · ms/gn · high. keyboard-driven = `billentyűvezérelt` · descriptive · high.
- AI provider → `szolgáltató` · descriptive (ms "szolgáltató") · high. endpoint = `végpont` · ms · high. API key =
  `API-kulcs` · ms · high. model = `modell` · ms · high.
- "Settings > AI" navigation path → `Beállítások > AI` (matches the settled Settings section names; AI kept) · high.
  Phrased as "itt: Beállítások > AI" in sentences to avoid suffixing the path.
- character encoding → `karakterkódolás`; the encoding dropdown placeholder/label uses the short `Kódolás` · ms
  ("karakterkódolás") · high. Encoding groups: Unicode (kept), Western = `Nyugati`. "(Detected)" = `(felismert)`.
- word wrap → `sortörés` (verb sense in copy) / status badge `tördelés` · ms ("word wrap" = sortörés) · high.
- tail (follow file, `tail -f` sense) → `Követés` (label), "automatikus követés" (tooltip) · descriptive, no pile term ·
  tentative. FLAGGED.
- streaming (viewer large-file mode) → `streamelés` · loanword kept, common HU dev usage, no pile term · tentative.
  FLAGGED.
- line (text line) → `sor` · gn ("üres sorokkal") · high. Counted-line plural keeps singular `sor` in both branches
  (Hungarian no-noun-pluralize-after-number rule). character = `karakter` · high.
- search match (a found hit) → `találat` · dc ("Találat: %d", "Az összes találat") · high. "No matches" =
  `Nincs találat`. Match position `{current} / {total}` (slash, matches HU "x / y" UI idiom). previous/next =
  `Előző`/`Következő`.
- case sensitive (search toggle) → `Kis- és nagybetűk megkülönböztetése` · dc ("Search is case sensitive") · high.
- regular expression → `reguláris kifejezés`; short toggle label `Regex` kept · dc · high.
- viewer (Cmdr's read-only file viewer) → `Megjelenítő` (matches the settled `viewer`/`megjelenítő` term); file viewer =
  `Fájlmegjelenítő`. raw view nudge: "view the actual <kind>" → "a tényleges <kind> nézetét".
- save panel (macOS native) → `mentési panel` · descriptive · tentative. "Save as file…" = `Mentés fájlként…`.
- reload (viewer file-changed) → `Újratöltés` · descriptive · high (distinct from `Frissítés` = refresh).
- Brand/format/units kept verbatim (legitimately identical-to-English in these three files): Cmdr, GitHub, Discord, PDF,
  Unicode, Regex, AI, `Cmdr AI {size}`, `{width} × {height}`, `?` (size-unknown glyph), Falcon-H1R-7B / Technology
  Innovation Institute / TII (proper names), `David Veszelovszki` + copyright year, `CMDR-ABCD-EFGH-1234` /
  `CMDR-XXXX-XXXX-XXXX` (key format examples), `sk-abc123…` / `sk-ant-abc123…` (key prefix examples),
  `https://api.example.com/v1`, getcmdr.com, gpt-4.1-mini, Apple Silicon, F7 / W / F / ⌘F / ⌘C / ⌘A key glyphs,
  `{placeholder}`-only and `100%` strings.

`{verb}`-style runtime-English tokens: none in these three files (no `transfer.json`-style operation-verb placeholders).

Settled while translating `indexing.json`, `downloads.json`, `errorReporter.json`, `shortcuts.json`, `mtp.json`,
`ui.json` (wave 1, 2026-06-21):

- log / log file → `napló` / `naplófájl` · ms ("log file" = naplófájl), matches the `Naplózás` settings section · high.
  log line = `naplósor`.
- error report (the named feature) → `hibajelentés` · descriptive (ms report = `jelentés`), treated as a product feature
  name not a bare "hiba" error label · high. "Send error report" = `Hibajelentés küldése`; the send button itself =
  `Jelentés küldése`.
- manifest (report metadata) → `Jegyzék` · ms ("manifest" = `jegyzékfájl`/`jegyzék`, XML-doc sense) · high.
- redact / scrub (privacy) → `kitakarás` (verb `kitakar`) · ms ("redact" = `kitakarás`) · high. "scrubbed before
  sending" rendered as "eltávolításra kerülnek küldés előtt".
- reference ID → `hivatkozási azonosító` · descriptive (ms reference = `hivatkozás`, ID = `azonosító`) · high.
- daemon → `démon` · ms · high. ptpcamerad kept verbatim (process name). system daemon = `rendszerdémon`; camera daemon
  = `kameradémon`.
- process (OS process) → `folyamat` · ms · high. "exclusive access" = `kizárólagos hozzáférés`.
- USB / USB device → `USB` (kept) / `USB-eszköz` · ms (USB kept) · high. "USB permission denied" =
  `USB-hozzáférés megtagadva`.
- udev rules → `udev-szabályok` · udev kept verbatim (Linux term, per en note), `szabály` = rule · high.
- Terminal (macOS app) → kept verbatim `Terminal` (Apple app name; not the Windows-Terminal `Terminál`) · mac · high.
  "terminal" generic (lowercase) = `terminál`.
- toast (here "notification") → rendered as `értesítés` in user-facing copy (the `buborék` term from the settings pass
  stays the internal label) · high.
- jump to (download/file) → `ugrás` · descriptive · high. "Jump to file" = `Ugrás a fájlhoz`.
- global shortcut (system-wide) → `globális parancs` / scope title `Globális` · descriptive · tentative. FLAGGED.
  "globally" = `globálisan`.
- modifier (key) → `módosítóbillentyű` · ms · high. combo (key combination) → `kombináció` · descriptive · high.
- register (a global hotkey) → `regisztrálás` (`Regisztrálva` / `Nincs regisztrálva`) · ms · high.
- Brief / Full mode (Cmdr view names) → `Rövid` / `Teljes` · descriptive (gn "brief"/"full" listings) · high. Reconciled
  across all files to `Rövid` (was split `Tömör`/`Rövid`; `Rövid` is the literal "brief" and dominates the catalog). The
  `mode`/`view` head matches the English per key: "Brief mode" = `Rövid mód`, "Brief view" = `Rövid nézet`, "Full
  mode/view" = `Teljes mód`/`Teljes nézet`. Don't use `Tömör` for this (it means "compact/concise"; reserved for "make
  more compact" = `Tömörebb` and "compressed" = `tömörített`).
- volume chooser → `Kötetválasztó`; main window = `Főablak`; About window = `Névjegyablak`; share browser =
  `Megosztásböngésző` · descriptive (compounds on settled terms) · high.
- Character Viewer (macOS) → `Karaktermegjelenítő` · descriptive, no direct mac hit (mac uses `Emodzsik és szimbólumok`
  for the picker), matches `megjelenítő` term · tentative. FLAGGED.
- Force Quit (macOS) → `Kilépésre kényszerítés` · mac (AppKit Menus "Force Quit…" = `Kilépésre kényszerítés…`) · high.
- App switcher / App windows (macOS) → `appváltó` / `Appablakok` · descriptive · tentative. FLAGGED.
- Mission Control, Spotlight, Spaces → kept verbatim (Apple feature names, not localized) · mac · high. So legitimately
  identical-to-English.
- input source switching → `beviteli forrás váltása` · descriptive (ms "input source" = `beviteli forrás`) · high.
- "no shortcut" / "(none)" → `Nincs billentyűparancs` / `(nincs)` · matches `Billentyűparancsok` settings section ·
  high. fixed (badge) = `Rögzített`.
- ETA abbreviations: seconds-left `{n} mp`, minutes-left `{n} p` (mp = másodperc, p = perc); "roughly" = `kb.`; "Almost
  done" = `Mindjárt kész` · descriptive HU abbreviations · high.
- Brand/units kept verbatim (legitimately identical-to-English in these six files): OK, App (`shortcuts.scope.app`),
  macOS, Cmdr, Finder, Spotlight, Mission Control, Spaces, MTP, ptpcamerad, udev, Terminal, USB, Android, Ctrl+C,
  ⌘/⌃/⌥/⇧ glyphs, the `{placeholder}`-only counter string.

Settled while translating `search.json`, `feedback.json`, `crashReporter.json`, `goToPath.json`, `transfer.json`,
`updates.json`, `lowDiskSpace.json`, `commandPalette.json`, `whatsNew.json`, `main.json`, `common.json`,
`notifications.json` (wave 1, 2026-06-21):

- crash → `összeomlás` · ms · high. crash report → `összeomlási jelentés` (report = `jelentés`, ms). "quit unexpectedly"
  = `váratlanul bezárult`. Report ID = `Jelentésazonosító`.
- startup disk → `indítólemez` · mac ("Startup Disk" = `Indítólemez`) · high. The macOS boot-volume term.
- restart (apply an update) → `Újraindítás` (label) / `indítsd újra` (verb, informal `te`) · mac (AppKit Menus "Restart"
  = `Újraindítás`) · high. "Restart to apply" = `Az életbe léptetéshez indítsd újra`.
- later (dismiss-for-now button) → `Később` · descriptive, common HU UI usage · high.
- What's new (post-update dialog) → `Újdonságok` · ms ("what's new" = `Újdonságok`) · high. "What's new in Cmdr" =
  `Újdonságok a Cmdrben`.
- changelog → `változásnapló` · ms ("changelog" = `változásnapló`) · high. "See full changelog" =
  `Teljes változásnapló megtekintése`.
- path (go-to-path feature) → `útvonal` · descriptive (ms "path" = `útvonal`) · high. "Go to path" = `Ugrás útvonalra`.
  To dodge vowel-harmony suffixing on the locale-formatted `{dir}`/`{requested}`/`{landed}` placeholders, paths sit in
  postposition/neutral slots: "A legközelebbi hely, ahová ugorhatsz: {dir}.", "ide hoztunk: {landed}".
- target (transfer destination, "already at the target") → `célhely` · descriptive (extends the settled `cél-`
  destination prefix) · high. "already at the target" = `már a célhelyen volt`.
- skipped (transfer outcome) → `kihagyva` · descriptive (distinct from the conflict-policy `Átugrás` button; this is the
  past-outcome participle) · high.
- "Show all in main window" (Search) → `Összes megjelenítése a főablakban` · descriptive (főablak settled in prior pass)
  · high.
- error prefix label (`updates.checkToast.errorPrefix`, "Error: {message}") → `Probléma: {message}` (the
  `error → Probléma` calm-voice rule, no bare "Hiba" label) · high.
- Dismiss (crashReporter/lowDiskSpace) → `Elvetés` (consistent with the prior `fileOperations` pass; MS gives `bezárás`
  but that collides with the settled Close = `Bezárás`) · tentative.

### transfer.json ICU plural/select notes

- Hungarian CLDR categories `one`/`other`, but the counted noun stays SINGULAR in BOTH branches (no
  pluralize-after-number): `{count, plural, one {fájl} other {fájl}}`, `{folders, plural, one {mappa} other {mappa}}`.
  The branches are written identically only because no other agreement word rides along; the ICU `other` branch is still
  required.
- The `{skipped, plural, one {was} other {were}}` was/were agreement in `transfer.fileOnly.mixedMove` collapses in
  Hungarian: the verb is `volt` regardless of count, so the second plural select is dropped from the sentence and only
  the noun-plural (still singular `fájl`) remains. Placeholder SET preserved (`{skippedText}`, `{skipped}`); the
  `{skipped}` token still drives the noun branch.
- `{verb, select, copy {…} other {…}}` rendered with the nominal `Másolás`/`Áthelyezés` for the opening label and the
  participles `másolva`/`áthelyezve` for the inline verb; the `{phrase}` fragment (from `transfer.movedPhrase`) is
  inserted after a colon ("Másolva: {phrase}.") so the reusable fragment stays grammatically standalone.

Brand/pure-placeholder kept verbatim (legitimately identical-to-English): `feedback.dialog.counter` ("{currentText} /
{maxText}", pure placeholders). Brands kept inline: Cmdr, macOS, GitHub, David, Enter (key name).

## Cross-file reconciliation (2026-06-21)

After all files were translated, a whole-catalog pass fixed drift the per-file fan-out left (the same English term
rendered differently across files). Decisions, so they don't get relitigated:

- **Ellipsis: single-char `…` everywhere.** The English source mixes `…` and `...` arbitrarily; Hungarian uses the
  typographic `…` (matches the `„…”` / native-date typography stance in `style.md`). All trailing-ellipsis values
  normalized to `…`.
- **Quotation marks: `„…”` (low-high), never English `"…"`.** Per `style.md`. e.g. `commands.handler.favoriteAdded` =
  `A(z) „{name}” …`, matching `shortcuts.section.alreadyBound` = `… „{command}”`.
- **`Brief` view → `Rövid`** (not `Tömör`): see the reconciled glossary entry above.
- **`Modified` (column/filter/chip) → `Módosítva`** uniformly (was split `Módosított` in the shortcuts filter). The
  `-va` participle is the column/state form used everywhere else.
- **`Don't show again` → `Ne jelenjen meg többé`** (was split with `…újra`).
- **`Endpoint URL` → `Végpont URL-címe`**, **`Example:` (placeholder lead-in) → `Példa:`** (not `Például:` = "for
  example"), **`On disk` → `Lemezen`**, **`Reset all to defaults` → `Összes visszaállítása alapértékre`** (matches
  `Összes kijelölése`), **`Go to latest download` → `Ugrás a legutóbbi letöltéshez`**, **`Press Enter to search` →
  `Nyomd meg az Entert a kereséshez`**, **`Tab limit reached` → `Elérted a lapok korlátját`**, **`Something went wrong`
  → `Valami nem sikerült`** (matches the `nem sikerült` calm-voice rule). All unified to one form across files.
- **Example email placeholder → `you@example.com`** verbatim everywhere (the en `@key` calls it a literal example;
  `te@pelda.hu` was a one-file localization that broke parity).

Forward-references confirmed resolved against the final files:

- crashReporter "Settings > Updates" = `Beállítások > Frissítések és adatvédelem` matches
  `settings.section.updatesAndPrivacy` (and `whatsNew.optOutToast`).
- All `Beállítások > AI` (ai.json) and `Beállítások > Billentyűparancsok` (fileExplorer) match the settled Settings
  section names.

`host` register split is intentional and correct in the final files: `gazdagép` only in errors.json connection-failure
prose; `gép`/`Gépnév` in the fileExplorer SMB browser and `commands.networkSelectHost` (`Hálózati gép`). `kiszolgáló` in
errors.json is the participle "hosting/serving" (not the noun "server" = `szerver`), so it doesn't violate
`server → szerver`. `settings.updates.errorPrefix` = `Hiba:` is correct (the en `@key` marks it dev/diagnostic, where
"Error" is allowed), distinct from the user-facing `updates.checkToast.errorPrefix` = `Probléma:`.

Settled while translating `queue.json` + the new pause/queue/background keys in `fileOperations.json`/`commands.json`
(transfer-queue feature, 2026-06-21):

- pause (a running transfer) → `Szüneteltetés` (button), `Szüneteltetve` (status/title) · double-commander (`Pau&se` =
  `Szünetel&tés`, `Paused` = `Szüneteltetve`), macOS (`szüneteltetés`) · high. The DC operations viewer is the direct
  parallel to Cmdr's queue window. "Pause all" = `Mindet szünetelteti` (DC `&Pause all` = `Mindet &szünetelteti`).
- resume (a paused transfer) → `Folytatás` · double-commander (`&Resume` = `Folytatás`), ms (`resume` = `folytatás`) ·
  high. "Resume all" = `Mindet folytatja`.
- queue (the transfer queue) → `sor` (`átviteli sor` = transfer queue) · double-commander (operations viewer `Queue` =
  `Sor`, `New queue` = `Új sor`), ms (`várólista`/`várakozási sor`) · high. DC's file-manager-native `Sor` beats MS's
  generic `várólista`. **SUPERSEDED for the window's NAME as of 2026-08-08** (English renamed it "Operation queue"; it
  is now `Műveleti sor` — see the dated block at the end of this file). The head `queue → sor` below still stands; only
  the `átviteli` modifier is retired. Window title `queue.windowTitle` = `Átviteli sor`; the command
  `commands.queueShow.label` = `Átviteli sor megjelenítése`; empty state "Nothing in the queue" = `A sor üres`. The
  progress-dialog "Queue" button (sends the transfer to the background and opens the queue window) = `Sorba` (short
  label, "into the queue"; mirrors DC `A&dd To Queue` = `Várakozási &sorba helyez`); its aria "Send to the transfer
  queue" = `Áthelyezés az átviteli sorba`.
- background / send to background → `háttér` (`a háttérben` = in the background) · double-commander ("Work in
  background" = `Háttérben futtatás`, "When application is in the background" = `Ha az alkalmazás a háttérben fut`), ms
  (`background` = `háttér`) · high. "Keep this running in the background" = `Hagyd futni a háttérben`; "Still running in
  the background" = `Tovább fut a háttérben`.
- queue-row status terms (`queue.row.status` select): queued = `Várakozik`, running = `Fut`, paused = `Szüneteltetve`,
  done = `Kész` (matches the settled Done = `Kész`), cancelled = `Megszakítva` (extends
  `cancel (running op) → megszakítás`), failed = `Nem sikerült befejezni` (the `nem sikerült` calm-voice rule, no bare
  "Hiba"/"sikertelen" label) · high. Row action labels reuse the running-transfer `megszakítás` for Cancel
  (`Megszakítás`), distinct from the dialog-close `Mégsem`.
- `queue.row.label` operation select reuses the settled nominal verbs
  (`Másolás`/`Áthelyezés`/`Törlés`/`Áthelyezés a Kukába`); "Working" fallback = `Folyamatban`.
- Counted-noun plurals keep the singular in both branches (Hungarian no-pluralize-after-number rule): `queuedToastCount`
  = `{# átvitel}` both branches; `selectedCount` = `{# kijelölve}` both branches (the `-ve` adverbial participle,
  matching `Kész`/`Megszakítva`).

Settled while translating the double-click-to-parent navigation keys (`settings.json` + `fileExplorer.json`, 2026-06-26;
re-validated against the reference pile):

- parent folder → `szülőmappa` · ms terminology (dedicated entry "parent folder" = `szülőmappa`), gn/xf (`szülőmappa`),
  Total Commander (`Szülő mappa`), Double Commander (`szülőkönyvtár`); kept for CATALOG CONSISTENCY · tentative. The
  whole shipped Cmdr catalog already uses `szülőmappa` for this concept: `commands.navParent.label` =
  `Ugrás a szülőmappára`, plus six `errors.json` suggestion strings (`Lépj a szülőmappába …`,
  `… az összes szülőmappával együtt`, `… írási hozzáférésed a szülőmappához`). These 14 new keys must NOT fork
  terminology — a user seeing `szülőmappa` in the menu but a different word in a settings toggle is worse than either
  consistent choice — so they reuse the catalog term. Inflects regularly (back-vowel `mappa`): illative
  `a szülőmappába`, allative `a szülőmappához`.
  - Pile note for a future full-catalog migration: macOS Finder (Tier 1) renders ITS term for this gesture as
    `tartalmazó mappa` (`Localizable.json`: "Go To Enclosing Folder" = `Ugrás a tartalmazó mappához`; "Navigates the
    front Finder window to its enclosing folder" = `… a tartalmazó mappájához navigálja`). Apple's word translates
    "enclosing folder", not the "parent folder" wording every other source (and Cmdr's English) uses, so it isn't a
    clean win for "parent folder" — but if Cmdr ever wants the Finder-native term, the ONLY split-free way to adopt
    `tartalmazó mappa` is a single migration of the whole `hu` catalog (`commands.navParent` + the `errors.json` six)
    together with these keys, never a piecemeal switch. Until then, `szülőmappa` stays.
- double-click → `dupla kattintás` (noun); verb `duplán kattint` · mac, ms · high. "Double-click the empty space"
  rendered conversationally (`te`): "Kattints duplán a … üres területére".
- hint (Cmdr's one-time educational tip notification, `doubleClickHint`) → `tipp` · descriptive, common HU UI usage ·
  high. The internal "hint shown" flag = `… tippje megjelent`.
- "Never do this again" (playful button that turns the gesture off) → `Soha többé` · deliberate playful, impersonal
  rendering matching the EN button's tone (avoids the `te` imperative "ne csináld", per labels-are-impersonal) ·
  tentative.
- "I like it" (primary keep-on button) / "Don't like it?" (prompt) → `Tetszik` / `Nem tetszik?` · natural HU, the
  impersonal "it pleases [me]" construction, parallel pair · high.
- breadcrumb segment tooltip "Click to navigate to {path}" → `Kattints ide az ugráshoz: {path}` · the locale-formatted
  `{path}` sits after a colon in a neutral slot so no Hungarian case suffix has to vowel-harmonize with an unknown
  runtime value (per style.md § Notes, the goToPath `{dir}` pattern) · high.
- "Navigation" (card heading / section half) → `Navigáció` · descriptive · high. Section "Navigation & file ops" =
  `Navigáció és fájlműveletek` (reuses the settled `File operations → Fájlműveletek`).
- pane → `panel` (confidence upgrade from `style.md`'s `tentative`): the orthodox two-pane pair confirms it directly —
  Total Commander (`az aktív panelről`, `A célpanelről`, `másik panelen`) and Double Commander (`a bal oldali panelen`,
  `&Panelra`). Now `high` for the two-pane sense. "pane background" = `a panel háttere` (`a panel hátterén`/`-re`).
- file list → `fájllista` (confidence upgrade from `style.md`/`listing`'s `tentative`): Double Commander confirms it
  (`Elérési út mező szerkesztése a fájllista felett`, `Váltás a bal és jobb oldali fájllista között`). Now `high`.
- row / file row → `sor` / `fájlsor` · ms terminology (`row` = `sor`, high), Double Commander (`one per row` =
  `soronként`) · high. A row in the file list, one representing a file. `fájlsor` is a transparent compound parallel to
  the settled `fájllista` and `naplósor` (log line). Used in `doubleClickPaneNavigatesToParent.description`: "not a file
  row" = `nem pedig egy fájlsor`. Distinct from `sor` = text line (viewer); same word, context disambiguates.

Copy revision (shorter wording, 2026-06-26): the double-click-to-parent label/description were re-shortened by David.
New EN "Double-click the pane background to go up a folder" →
`Dupla kattintás a panel hátterére a szülőmappába lépéshez` (nominal/no-direct-address, matching the other
`settings.behavior.*.label` values like `Ugrás a legutóbbi letöltéshez`; reuses `panel háttér` + `szülőmappa`). New EN
"That''s the empty space around the file list, not a file row." →
`Ez a fájllista körüli üres terület, nem pedig egy fájlsor.` (`Ez` refers back to the pane background named in the
label; reuses `fájllista` + the new `fájlsor`).

- preset (value in a settings-picker dropdown) → előbeállítás; "back to presets" → "Vissza az előbeállításokhoz"
  (allative -hoz, vowel-harmonized) · Double Commander hu ("előbeállítás": "módosított előbeállítással", "a …
  előbeállítást") · high

Settled while translating the FAT32-file-too-large keys (`errors.write.filesTooLargeForFilesystem.*` +
`fileOperations.errorDialog.tooLargeAndMore`, 2026-06-30):

- "too large for [destination]" (the over-the-filesystem-limit error) → `túl nagy ehhez a meghajtóhoz` · mac Finder
  (Tier 1) directly: `A fájl túl nagy a célhoz` (the file-too-large-for-destination title) and
  `…nem másolható, mert túl nagy a kötet formátumához képest` (the copy-blocked-by-volume-format message), also Total
  Commander (`A(z) "%s" fájl mérete túl nagy a cél fájlrendszer számára!`) and Double Commander
  (`Ez túl nagy a céleszközön…`) · high. macOS uses `a célhoz` (the destination); Cmdr's EN says "this drive" so we
  render `ehhez a meghajtóhoz` (drive = `meghajtó`, settled). `túl nagy` is the unanimous pile rendering of "too large".
- "formatted as FAT32 / drive formatted as exFAT" → `FAT32 formátumú` / `exFAT formátumú meghajtó` · the `…formátumú`
  ("of … format") construction is pile-attested (`ismeretlen formátumú`, `rossz formátumú` in the corpora) and parallels
  mac Finder's `a kötet formátumához` · high. `FAT32`/`exFAT` kept verbatim (filesystem-format names, per the en `@key`
  note).
- "larger than {maxSize}" → `{maxSize}-nál nagyobb` (comparative `-nál` suffix on the size placeholder). Normally the
  style guide forbids suffixing a placeholder (vowel harmony with an unknown value), but here the value domain is
  CONSTRAINED to a formatted byte size whose unit is always back-vowel when pronounced (B = bájt, kB = kilobájt, MB =
  megabájt, GB = gigabájt, TB = terabájt), so `-nál` (never `-nél`) is always the correct harmony. The suffix renders
  just after the colorized size span (`…GB</span>-nál`), uncolored, which is the wanted typography. Double Commander's
  `nagyobb mint 4GB` confirms users see inline size comparisons. · high.
- limit (the filesystem size limit) → `korlát`; "has no such limit" → `amelynek nincs ilyen korlátja` · mac/ms (`korlát`
  16×, possessive `méretkorlátja`/`összegkorlátja` attest the `-ja` possessive form) · high.
- "and {countText} more {file/files}" (trailing over-limit-list line) →
  `és {countText} további {count, plural, one {fájl} other {fájl}}` · mac Finder directly:
  `…a(z) „^1” és ^0 további elem…` ("…„^1” and ^0 more items…") — same `és {count} további {noun}` shape, count BEFORE
  `további`, and the noun stays SINGULAR after the number (no-pluralize rule), so both ICU plural branches are `fájl` ·
  high.
- preset (value in a settings-picker dropdown) → előbeállítás; "back to presets" → "Vissza az előbeállításokhoz"
  (allative -hoz, vowel-harmonized) · Double Commander hu ("előbeállítás": "módosított előbeállítással", "a …
  előbeállítást") · high

Settled while translating the copy/delete-dialog polish keys (`fileOperations.json`, 2026-06-30):

- action (what a control chooses; screen-reader label `transferDialog.operationAria`) → `Művelet` · ms terminology
  ("action" = `művelet`, Noun), macOS Finder ("This action cannot be performed." = "Ez a művelet nem hajtható végre.") ·
  high. Matches the settled `File operations → Fájlműveletek` (művelet = operation/action). Sentence case.
- "Scanning…" (spinner tooltip + SR label WHILE counting selected items, `shared.scanningTooltip`) → `Átvizsgálás…` ·
  in-file consistency with `transferProgress.stageScanning` = `Átvizsgálás`, glossary `scan (index) → átvizsgálás`, ms
  ("scan" = examine files/data = `vizsgál`) · high. Ellipsis `…` kept (single char, per the typography reconciliation).

Settled while translating the destination-will-be-created warning keys (`fileOperations.json`, 2026-06-30):

- "This folder doesn't exist yet. Cmdr will create it during the copy/move." (yellow inline warning under the
  destination box when the typed dest folder doesn't exist, `transferDialog.targetWillBeCreatedCopy`/`…Move`) →
  `Ez a mappa még nem létezik. A Cmdr létrehozza a másolás során.` / `… az áthelyezés során.` · `nem létezik` for
  "doesn't exist" is Total Commander / Double Commander's exact phrasing for a missing target dir ("A(z) „%s” könyvtár
  nem létezik. Létrehozza?"); `még nem létezik` adds the "yet"; "during the copy/move" = `a másolás során` /
  `az áthelyezés során` (Double Commander "másoláskor"/"… során"); reuses settled `folder → mappa`, `copy → másolás`,
  `move → áthelyezés`, `create → létrehoz` (`létrehozza` = definite conj., the "it" object folded in) · high. Two
  literal sentences per the en `@key` note (operation-specific verb, no ICU select). Brand `A Cmdr` bare as subject
  (consonant onset → article `a`).
- **queue.row.label progress arms (rename / create folder / create file)** · `Átnevezés` / `Mappa létrehozása` /
  `Fájl létrehozása` · verbal-noun style of the sibling arms (Másolás, Áthelyezés); Nautilus ("átnevezése", "…
  létrehozása"), reuses settled `rename → átnevezés`, `create → létrehoz`, `mappa`/`fájl` · high

Settled while translating the archive-browsing feature keys (`errors.json`, `fileExplorer.json`, `fileOperations.json`,
`settings.json`, `viewer.json`, `queue.json`, 2026-07-05):

- archive (a zip/tar/7z Cmdr browses like a folder) → `archívum` · macOS Finder (Tier 1) directly: `CompressWithOptions`
  has "Zip archive" = `Zip archívum`, "Apple Archive" = `Apple-archívum`, "CPIO archive" = `CPIO-archívum`; the shipped
  Cmdr catalog already uses it (`settings.fileViewer.suppressBinaryWarning.description` = "…archívumot…") · high. Beats
  Total Commander's `tömörített fájl` (compressed file) on the macOS-Finder-wins rule + existing-catalog consistency.
  Inflects regularly (back vowel `archívum`): accusative `archívumot`, elative `archívumból`, superessive `archívumon`,
  plural `archívumok`.
  - Orthography: a lowercase format token compounds with a hyphen (`zip-archívum`, `tar-`/`7z-archívum`); a capitalized
    proper-name-style token takes a space, matching macOS's own `Zip archívum` (so heading "Zip archives" =
    `Zip archívumok`, but in-sentence lowercase "zip archives" = `zip-archívumokat`). "a zip file" stays `zip fájl`
    (macOS "Zip file" = `Zip fájl`).
- extract (unpack files from an archive) → `kicsomagol` (verb), `kicsomagolás` (noun) · Total Commander (Cmdr's two-pane
  lineage) throughout its archive UI ("Fájl kicsomagolása", "Kicsomagolás:", and the tip "kattints rá kétszer, mint egy
  mappára" — its exact parallel to Cmdr's browse-an-archive-like-a-folder gesture) · high. "browses and extracts …
  archives" = `böngészi és kicsomagolja a … archívumokat`.
- app bundle / bundle / package (.app, .bundle, .framework) → `csomag`; "app bundles" = `alkalmazáscsomagok` · macOS
  Finder "Show Package Contents" = `Csomag tartalmának megjelenítése` (package = `csomag`), Microsoft terminology bundle
  = `csomag` · high. Cmdr's UI says "App bundles" (not "packages"), so the transparent compound `alkalmazáscsomag` names
  the .app/.bundle/.framework category (keys 16 & 19 both `Alkalmazáscsomagok`); the generic standalone "bundle" (aria
  "Open archive or bundle") is bare `csomag`.
- browse (step inside an archive/bundle and list it like a folder) → `böngészés` (noun) · Cmdr catalog already uses the
  `böngész-` root (`git böngésző`, `Megosztásböngésző`); MS `tallózás` is the file-picker sense, not this one · high.
  "Browse like a folder" = `Böngészés mappaként` ("as a folder", matching Total Commander's "mint egy mappára").
  Segmented cell `settings.archives.opt.browse` = `Böngészés`.
- "Open with default app" → `Megnyitás az alapértelmezett appban` · shipped catalog
  (`fileExplorer.quickLookHint.enterOpens` = "…az alapértelmezett appban", `settings.fileViewer…` = "a társított
  appban") · high. `app` kept as loanword (glossary). Segmented cell `settings.archives.opt.open` = `Megnyitás`.
- Ask (Enter-behavior option: pop up a browse/open prompt) → `Rákérdezés` (nominal, short segmented cell) · descriptive,
  common HU UI usage; nominal form fits the one-cell control (the longer önözés `Mindig kérdezzen` of
  `allowFileExtensionChanges.opt.ask` is for a wider control) · tentative. "or ask each time" (description) =
  `vagy rákérdezés minden alkalommal`.
- Configure… (menu item opening Settings for this format) → `Konfigurálás…` · Microsoft terminology (`konfigurálás`
  throughout) · high. Chosen over `Beállítás…` to avoid colliding with Settings = `Beállítások`. Single-char ellipsis.
- "Read-only archive" → `Csak olvasható archívum` · reuses settled `read-only → csak olvasható` + `archívum` · high.
- "There's no trash inside an archive." → `Egy archívumon belül nincs Kuka.` · reuses settled `trash → Kuka`
  (capitalized, the Trash feature). Followed in the same banner by "removed from the zip for good" =
  `Ezek az elemek véglegesen törlődnek a zipből.` (items = `elemek`, macOS Finder "Compress Items" = "Elemek
  tömörítése"; `for good → véglegesen`; `zipből` elative, front harmony) · high.
- "Editing archive" (queue.row.label `archive_edit` arm: changing a zip's entries) → `Archívum szerkesztése` ·
  verbal-noun style of the sibling arms; reuses `edit → szerkesztés` (catalog `commands.fileEdit.label`) + `archívum` ·
  high.
- damaged / corrupt → `sérült`; encrypted → `titkosított` · shipped catalog (`errors.git.corruptRepo` = "sérültnek
  tűnik", `errors.provider.veraCrypt.*` = "titkosított kötet") + macOS Finder ("Encrypted" = "Titkosítva") · high.
- No `sameAsSourceJustification` needed in this batch: every one of the 28 values differs from English (the segmented
  cells Böngészés/Megnyitás/Rákérdezés all translate; no brand-only or unit-only values here).

Settled while translating the paste-clipboard-as-a-file keys (`settings.json` + `fileExplorer.json`, 2026-07-07):

- paste (the ⌘V action / pasting clipboard content) → `beillesztés` (verbal noun), inflects `beillesztve` (adverbial
  participle for the done-toast) · macOS AppKit `MenuCommands` directly ("Paste" = `Beillesztés`), Total Commander
  (`&Beillesztés`), reuses the settled `clipboard → vágólap`, `copy/cut/paste = Másolás/Kivágás/Beillesztés` · high. The
  toast done-status `beillesztve` matches the sibling `fileExplorer.clipboard.copied` = "… másolva" participle style.
- clipboard content (as a compound in the settings label) → `vágólaptartalom` (vágólap + tartalom) · transparent
  compound on the settled `clipboard → vágólap` + `content → tartalom` (`dirSize.contentLabel` = `Tartalom`) · high.
  Label `settings.fileOperations.pasteClipboardAsFile.label` = `Vágólaptartalom beillesztése fájlként` (verbal-noun,
  article-free, matching sibling labels like `Repozitóriumcímke megjelenítése`; "as a file" = `fájlként`, essive-modal
  `-ként`, invariant so no harmony worry). The description reuses the archives-picker frame "What Enter does…" =
  `Mit tesz a(z) …`: `Mit tesz a ⌘V egy mappában, amikor a vágólapon szöveg, kép vagy PDF van másolt fájlok helyett.`
  (article `a ⌘V`, matching the catalog's `a ⌘C`/`a ⌃⌥⌘J` shortcut-glyph articles; no comma before `vagy`, per HU
  punctuation and the `suppressBinaryWarning` sibling).
- image → `kép` · macOS AppKit `Accessibility` ("Image" = `Kép`) · high. text → `szöveg` · macOS AppKit `Services`
  ("Text" = `Szöveg`) · high. PDF kept verbatim (format name).
- paste-as-file settings options (radio/segmented, `pasteClipboardAsFile.opt.*`) rendered NOMINAL for register
  consistency with the archives-picker segmented cells (`Böngészés`/`Megnyitás`/`Rákérdezés`) and the queue verbal-noun
  arms, NOT the önözés form of the immediate sibling `allowFileExtensionChanges.opt`: "Do nothing" = `Nincs művelet`
  (reuses settled `action → művelet`; Double Commander's `Ne csináljon semmit` is önözés, not matched here), "Create
  file" = `Fájl létrehozása` (reuses `create → létrehoz` + `fájl`, mirrors `queue.row.label` create arm), "Create and
  rename" = `Létrehozás és átnevezés` (reuses `rename → átnevezés`; keeps the English's terse drop of "file" in the
  combined arm) · high.
- pasted-as-file done toast (`fileExplorer.clipboard.pastedAsFile`, ICU select on `{kind}`) →
  `A vágólap {kind, select, image {képe} pdf {PDF-je} other {szövege}} fájlként beillesztve: {filename}` · the branch
  words carry the possessive suffix (`képe`/`PDF-je`/`szövege` = the clipboard's image/PDF/text) so "A vágólap X" is
  grammatical; the uncontrolled `{filename}` sits after a colon in a neutral slot (no case suffix to vowel-harmonize
  with an unknown runtime value, per style.md § Notes) · high. Branch NAMES `image`/`pdf`/`other` kept verbatim.
  `PDF-je`: possessive `-je` on the front-vowel-pronounced abbreviation (pé-dé-ef), hyphen per abbreviation rule.
- No `sameAsSourceJustification` needed in this batch: all 7 values differ from English (`pastedAsFileSettings` =
  `Beállítások` differs from "Settings"; no brand-only or unit-only values).

Settled while translating the archive-password dialog keys (`fileOperations.archivePassword.*`, 2026-07-08):

- password-protected → `jelszóval védett` · TC/DC hu phrasing + macOS · high. Body: "A(z) <archive>{name}</archive>
  jelszóval védett."
- password (noun) → `Jelszó` · macOS/MS · high.
- unlock (button + verb) → button `Feloldás`; verb `feloldásához` / `feloldotta` · macOS AppKit ("Feloldás") · high.
- archive (input label) → `Archívum` (input aria-label "Archívum jelszava") · settled hu glossary · high.
- ACCUSATIVE HEAD-NOUN: the retry body attaches the accusative to a `fájlt` head, "… nem oldotta fel a(z)
  <archive>{name}</archive> fájlt", so no case suffix ever lands on the uncontrolled runtime `{name}` (per `style.md` §
  Notes: never vowel-harmonize a suffix onto an unknown value). Same trick the paste-toast uses with the colon slot.

Settled while translating the Compress feature:

- compress (verb / control label) → `Tömörítés` (verbal noun) · Finder `hu/macOS` ("Elemek tömörítése",
  `Compress ${sources}` → "${sources} tömörítése") · high. Used for `commands.fileCompress.label`, `toggleCompress`,
  `confirmCompress`, and `titleVerbOnly`; `titleWithCounts` uses the possessive `tömörítése` to match the sibling
  `másolása`/`áthelyezése`.
- compressing (progress form) → `Tömörítés` (hu reuses the noun form for the -ing state, as `Másolás`/`Áthelyezés` do) ·
  high. `scanTitleCompress` = "Ellenőrzés a tömörítés előtt…".
- compressed (result toast) → `tömörítve` (adverbial participle) · mirrors `transfer.split.clean` ("Másolva: {phrase}")
  · high.
- replace (overwrite warning) → `lecseréli` · Finder `Replace` → "Kicserélés", verb form `lecseréli` · high.
  `targetWillBeOverwritten` = "Már van itt egy ilyen nevű fájl. A Cmdr lecseréli."
- archive (name) → `archívum` · settled hu glossary + Finder "Zip archívum" · high. `.zip` in straight double quotes;
  the `-re` case suffix attaches to the quoted literal (".zip"-re) not to a runtime value.
- compression level (slider label) → `Tömörítési szint` · TC `hu` "Tömörítési arány (0-9)" (arány = ratio); `szint`
  (level) chosen for the 1–9 step slider, standard hu 7-Zip term · high. `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `Gyorsabb` · TC `hu` "Leggyorsabb tömörítés (1)" (root `gyors`) · high. Marks
  quicker packing, not app speed. `.faster`.
- smaller (slider high end, level 9) → `Kisebb` · comparative of `kis`, pairs with `Gyorsabb`; marks the smaller output
  file (TC `hu` high end "Maximális tömörítés") · high. `.smaller`.
- No `sameAsSourceJustification` needed: all values differ from English.

Settled while translating the Operation log feature (`operationLog.json` + `commands.logOperationLog.*`, 2026-07-09):

- operation log (the feature / dialog title / command) → `Műveletnapló` · REUSED verbatim from the already-shipped
  `settings.section.operationLog` = `Műveletnapló` · high. Transparent compound on settled `operation/action → művelet`
  (Fájlműveletek) + `log → napló`. Do NOT fork it — the settings section and the dialog name the same feature.
- roll back / rolling back / rolled back / rollback (the operation-log reversal action + statuses) → `visszagörgetés`
  (verb `visszagörget`) · RECONCILED to the shipped `fileOperations.transferProgress.*` rollback strings
  (`titleRollingBack` = "Visszagörgetés…", `conflictRollback` = "Visszagörgetés", `rollbackUnavailableTooltip`,
  `smbNativeNote`), which render the SAME rollback engine that this dialog surfaces · high (up from the earlier
  `tentative`/FLAGGED transfer-cleanup entry). Same English word ("roll back") + same engine → same term, so the dialog
  must not fork it. NOT `visszavon`: the shipped `settings.operationLog.intro` uses `visszavonhatod` because its EN
  source says "undo actions" (a different English word in prose), not the status-term "roll back"; and NOT MS's
  `visszaállítás` (Tier-2 "roll back = to reverse changes"), which is overloaded with reset/revert/restore. Status forms
  derive cleanly: "Can roll back" = `Visszagörgethető` (potential adjective), "Can''t roll back" =
  `Nem görgethető vissza` (negation detaches the coverb), "Rolling back" = `Visszagörgetés folyamatban` (state
  descriptor; the shipped `Visszagörgetés…` is the live dialog-title variant), "Rolled back" = `Visszagörgetve`
  (adverbial `-ve` participle, matching the `Kész`/`Megszakítva`/`Kihagyva` state style), "Partly rolled back" =
  `Részben visszagörgetve`.
- Lifecycle statuses reuse settled terms verbatim: queued = `Várakozik`, running = `Fut` (both from `queue.row.status`),
  done = `Kész`, canceled = `Megszakítva` (extends `cancel (running op) → megszakítás`). "Didn''t finish" (the softened
  EN for status/outcome `failed`) → `Nem fejeződött be` (neutral intransitive "didn''t finish", no bare
  "hiba"/"sikertelen"; distinct from `queue.row.status` failed = `Nem sikerült befejezni`, which translated the harsher
  EN "Failed"). Per-item outcomes: skipped = `Kihagyva` (settled), rolledBack = `Visszagörgetve`.
- operation summaries (`summary.*`) use the verbal-noun naming style (possessive `-ása/-ése`), matching the
  `queue.row.label` arms and macOS Finder "Elemek tömörítése": "Copied N items" = `{countText} elem másolása`, move =
  `… áthelyezése`, delete = `… törlése`, trash = `… áthelyezése a Kukába` (settled), rename = `… átnevezése`,
  createFolder = `{countText} mappa létrehozása`, createFile = `{countText} fájl létrehozása`, compress =
  `… tömörítése`. Counted noun stays SINGULAR in both plural branches (`elem`/`mappa`/`fájl`); `{countText}` kept in
  every branch. archiveEdit "Edited an archive" = `Archívum szerkesztése` (matches `queue.row.label` archive_edit
  verbatim), archiveExtract "Extracted an archive" = `Archívum kicsomagolása` (`extract → kicsomagol`, settled). "and N
  more items" = `és {countText} további elem` (macOS Finder `és ^0 további elem` pattern, singular noun).
- AI client (external AI app over Cmdr''s automation interface, provenance label) → `AI-kliens` · MS terminology (client
  = "an entity, such as a device or program, that connects to another entity over a network" = `kliens`) · high.
  Provenance siblings: "You" = `Te` (informal `te` register), "Agent" (Cmdr''s own AI) = `Ágens` (settled
  `agent → ágens`).
- items = `elem` (settled), "history" = `előzmények` (matches `settings.operationLog` `Előzmények megőrzése` +
  `átnézheted az előzményeidet`); "Couldn''t load…" body uses the settled `nem sikerült` calm-voice rule.
- No `sameAsSourceJustification` needed: all values differ from English (`AI-kliens`/`Te`/`Ágens` all translate).

Settled while translating the Ask Cmdr feature (`askCmdr.json` + the
`settings.askCmdr.*`/`settings.advanced.logLlmCalls.*`/`settings.section.askCmdr`/`commands.askCmdrToggle.*` keys,
2026-07-13):

- chat (a conversation with the AI) → `csevegés` · MS terminology (`chat` noun = `csevegés`, verb `cseveg`) · high. No
  macOS tier (Apple doesn''t localize a generic "chat" term; the one `macOS/` hit is the `iChat` brand, not useful).
  Plural UI section "Chats" → `Csevegések`. "New chat" (nominal button, matches `Új mappa`/`Új fájl`/`Új lap`) →
  `Új csevegés`.
- "Ask about your files" / "Ask about your files…" (empty-state heading + composer placeholder) → `Kérdés a fájljaidról`
  / `Kérdés a fájljaidról…` · reconciled to the shipped SEARCH-PLACEHOLDER convention: NOMINAL, not `te` imperative
  (`commandPalette.searchPlaceholder` = "Parancsok keresése…", `settings.sidebar.searchPlaceholder` = "Beállítások
  keresése…", both nominal "X keresése" not "keress X-et") · high. This settles the register question for Ask Cmdr
  specifically: its input placeholders and small empty-state header follow the established placeholder pattern, NOT the
  bigger warm-CTA `te` register used for full-screen onboarding headings (see the consent-title entry below for the
  contrasting case). "Search chats" / "Search your chats…" (aria + placeholder) → `Csevegések keresése` /
  `Csevegések keresése…`, same nominal "X keresése" pattern (`commandPalette.searchAriaLabel`,
  `commands.searchOpen.label`, `viewer.search.ariaLabel` all confirm "Search X" → "X keresése" universally in this
  catalog).
- "Talk to Cmdr about your files" (consent-screen welcome heading, explicitly "Warm and inviting" per the en `@key`) →
  `Beszélgess a Cmdrrel a fájljaidról` · `te` imperative, reconciled to the onboarding big-heading register (
  `onboarding.stepAi.title` "Now, let''s talk AI" → "Most pedig beszéljünk az AI-ról", `onboarding.stepBeta.title` "Help
  improve Cmdr!" → "Segíts jobbá tenni a Cmdrt!") rather than the placeholder-nominal register above · high. The split
  is deliberate: a standalone warm welcome/consent heading reads differently from a small empty-state title or a search
  box. `Cmdrrel` = Cmdr + comitative `-vel`, assimilated to `-rel` after the final `r` (standard Hungarian assimilation,
  matches `vízzel` from `víz+vel`).
- Close X (aria label for closing the rail) → `X bezárása` · REUSED verbatim pattern (`commands.volumeClose.label` =
  "Kötetválasztó bezárása", `commands.aboutClose.label` = "Névjegyablak bezárása", `commands.paletteClose.label` =
  "Paletta bezárása", `viewer.search.ariaLabel`-sibling `viewer.search.close` = "Keresés bezárása") · high. "Close Ask
  Cmdr" → `Az Ask Cmdr bezárása` (article `Az` before the vowel-initial "Ask", per the `A(z)` rule).
- stop (button that stops the assistant mid-answer) → `Leállítás` · MS terminology (`stop` verb = `leállítás`
  noun/`leállít` verb) · high. Distinct from the running-transfer `cancel → megszakítás` (a different feature); no clash
  since Ask Cmdr has no transfer-cancel concept.
- thinking (status shown while the assistant reasons before replying) → `Gondolkodik…` · MS gives the noun
  `gondolkodás`; rendered as the 3rd-person-present verb to match the catalog''s established live-status-word pattern
  (`queue.row.status` `Fut`/`Várakozik`) · tentative (no exact "Thinking…" precedent in the pile; this is a novel
  AI-chat-status concept).
- AI tool-call status chips (`askCmdr.tool.*.doing`/`.done` pairs — narrating what the read-only assistant is doing/did:
  reading app state, listing a folder, finding folders, checking importance, listing drives, searching/reading the
  operation log): rendered as VERBAL NOUN (doing, ongoing) + adverbial `-va/-ve` PARTICIPLE (done, completed) pairs on
  the SAME object, e.g. `A nézeted ellenőrzése` / `A nézeted ellenőrizve`, `Egy mappa listázása` / `Egy mappa listázva`
  · descriptive, no direct pile precedent (a genuinely novel AI-agent narration concept), but the CONSTRUCTION reuses
  two already-shipped Cmdr conventions rather than inventing a new one: the verbal-noun progress-status style (matches
  `fileOperations` progress titles `Átvizsgálás`/`Tömörítés` and the settled "Checking for conflicts" →
  `Ütközések keresése`), and the participle done/state style (matches `selectedCount` = `{# kijelölve}`,
  `fileExplorer.clipboard.copied` = "… másolva", `queue.row.status` `Szüneteltetve`/`Megszakítva`) · tentative, FLAGGED
  (kept genuinely distinct doing/done text on purpose — a screen reader or a scrollback reading the tool-call history
  needs the two states to read differently, not just rely on a spinner/checkmark icon). "Finding X" / "Found X" pairs
  use the `keres`/`talál` (search/find) verb pair instead (`A legnagyobb mappák keresése` / `… megtalálva`,
  `A fontos mappáid keresése` / `… megtalálva`, `Egy művelet megkeresése` / `Egy művelet megtalálva`) since English
  itself distinguishes "finding" from "checking". "Searching your file history" reuses the settled `scan → átvizsgálás`
  term instead (`A fájlműveleti előzményeid átvizsgálása` / `… átvizsgálva`) since scanning a log is the same concept as
  scanning a filesystem. Fallback `unknown.doing` "Working" → `Folyamatban` (REUSED verbatim from the shipped
  `queue.row.label` "Working" fallback). `unknown.done` "Used a tool" → `Egy eszköz használva` (`tool` = internal AI
  function = `eszköz`, generic dev sense).
- "file history" (Ask Cmdr''s casual name for what the operation log holds, used in the empty-state hint and the
  operationsList tool status) → `fájlműveleti előzményeid` · REUSES the settled `history → előzmények` term from the
  Operation Log pass (same underlying concept: past copies/moves/deletes/renames) rather than inventing a new calque,
  for cross-feature consistency · high.
- attach (verb, attaching a file/folder to a question) / attachment (noun) → `csatol`/`csatolás` (verb/verbal noun) /
  `Melléklet` (noun) · MS terminology: attach has TWO distinct senses in the tbx — `csatlakoztat` (hardware/device
  connect) and `csatol` (file/document attach, the one Cmdr wants) — and `attachment` = `Melléklet` (the classic
  email-attachment noun, reused generically) · high. NOTE: `csatol` is the SAME lemma the glossary already uses for
  `mount → csatolás` (SMB/MTP volumes), but this mirrors the MS source material itself (macOS uses `csatol` for mount,
  MS uses `csatol` for file-attach) and mirrors real Hungarian software (e.g. "csatolt fájl" vs "csatolt kötet");
  context (Settings vs the chat composer) disambiguates, same as English overloads "attach/mount" differently but both
  translate through the one verb. No clash in practice: the CONSENT copy''s "connected drives" deliberately uses
  `csatlakoztatott` (connected, from `csatlakozik`) instead, sidestepping any ambiguity there. "Remove attachment" →
  `Melléklet eltávolítása` (REUSES the settled "X eltávolítása" removal pattern, e.g. `downloads` "Remove download" =
  "Letöltés eltávolítása").
- drop (drag-and-drop release, "Drop to attach" overlay) → `Ejtsd ide a csatoláshoz` · Double Commander hu ("Húzd és
  ejtsd" = drag and drop, confirming `ejt` = drop) · high.
- archive / unarchive (hide/restore a chat from the active list — DISTINCT from the zip-archive `archívum` noun already
  in this glossary) → `Archiválás` / `Archiválás visszavonása` · MS terminology (`archive` verb = `archivál`, distinct
  termEntry from the zip-archive `Archívumok` noun sense) · high for `Archiválás` (matches real-world Hungarian mail-app
  usage, e.g. Gmail hu "Archiválás"); `tentative` for `Archiválás visszavonása` (no direct "unarchive" pile hit; the "X
  visszavonása" undo-toggle pattern is attested for reversible actions, e.g. macOS "Módosítások visszavonása" = "undo
  changes"). Archived-chat BADGE (adjectival, not a button) → `Archivált`, matching the settled adjectival badge style
  (`fixed (badge) → Rögzített`).
- selection (Ask Cmdr''s "Ask about selection" attach button) → REUSES the settled `selection → kijelölés` term:
  `Kérdés a kijelölésről` (nominal, matches the "Ask about your files" → "Kérdés a fájljaidról" pattern above).
- quota → `kvóta` · MS · high. database → `adatbázis` · MS · high. dashboard → `irányítópult` · MS · high. spending
  (settings section heading) → `Kiadás` · MS (`spending` = `kiadás`) · high. bill (verb, "your provider bills you") →
  `számláz` · MS · high. consent (backing concept, not a literal surface string here) → `hozzájárulás` · MS · high (not
  directly used verbatim in any translated value; the consent-screen strings are full sentences, not the bare noun).
- estimate/estimated (cost) → `becslés`/`becsült` · descriptive, common vocabulary; MS''s only "estimate" hit
  (`árajánlat`) is the commercial-quote sense, wrong for Cmdr''s "approximate token cost" sense, so rejected · tentative
  for the specific UI collocations ("about {amount}" → `kb. {amount}`, REUSING the settled ETA abbreviation
  `roughly → kb.`; "cost unknown" → `költség ismeretlen`; "free, on-device" → `ingyenes, az eszközön`, matching the
  shipped `ai.local.notInstalled` "runs entirely on-device" → "teljes egészében az eszközödön fut" for the on-device
  sense, generic/no-"your" since English itself drops "your device" for the terse "on-device").
- "This chat''s usage" (cost-footer aria label) → `E csevegés felhasználása` · MS (`usage` = `felhasználás`) · high.
- Provider/model labels ("Cloud AI", "Local LLM", "Off") inserted via the `{provider}` placeholder are NOT translated
  here — they come pre-localized from `settings.ai.provider.opt.*` (`Felhő-AI`, `Helyi LLM`) at runtime; only the
  surrounding sentence needed translating.
- `Turn on Ask Cmdr` (shared string, both the consent accept-button and the settings turn-on button, same `sourceHash`)
  → `Ask Cmdr bekapcsolása` · REUSES the shipped "X bekapcsolása" turn-on pattern (`indexing.firstConnect.enable` =
  "Indexelés bekapcsolása", `fileExplorer.navigation.driveIndex.menuEnable` = "Indexelés bekapcsolása ehhez a
  meghajtóhoz") · high. The generic sentence form "Turn on an AI provider in settings/Settings › AI to use Ask Cmdr"
  instead reuses the imperative pattern from `ai.translateError.off.body` ("Kapcsolj be egy szolgáltatót itt:
  Beállítások > AI, hogy használd az AI-keresést.") — note `settings.askCmdr.*` keeps the EN source''s literal `›` glyph
  (a different navigation-arrow character than `ai.json`''s plain `>`); both are kept verbatim as punctuation, never
  translated.
- "Something went wrong on the provider''s side" / "That took too long" / "The reply didn''t finish" / budget-exhausted
  error copy all REUSE settled calm-voice building blocks: `Something went wrong → Valami nem sikerült`,
  `Try again? → Próbáld újra?` (matches `feedback.dialog.softFailure`), `didn''t finish → nem fejeződött be` (matches
  the Operation Log `Didn''t finish` status), `limit → korlát` (possessive `korlátját`, matches the FAT32 pass''s
  `méretkorlátja`/`összegkorlátja` possessive evidence).
- "Chat with an AI about your files, drives, and history" (shared verbatim EN string:
  `commands.askCmdrToggle.description` AND the first sentence of `settings.askCmdr.intro`) →
  `Csevegj egy AI-val a fájljaidról, meghajtóidról és előzményeidről` · `te` imperative, matching the EN''s own
  imperative mood and the `commands.feedbackSend.description` precedent ("Tell the maker of Cmdr what you think…" →
  "Mondd el a Cmdr készítőjének, mit gondolsz…", also imperative) rather than the more common 3rd-person
  `commands.*.description` style (`Megnyitja…`, `Elküldi…`) · high, reused verbatim in both places since it''s the
  identical English string.
- `sameAsSourceJustification` recorded for the three verbatim "Ask Cmdr" product-name occurrences (`askCmdr.title`,
  `settings.section.askCmdr`, `commands.askCmdrToggle.label`): kept per the en `@key` note that this is the product
  name. Everything else in this batch differs from English (all 96 keys across `askCmdr.json`, `settings.json`'s 17 Ask
  Cmdr/LLM-logging keys, and `commands.json`'s 2 keys translate; no other identical-to-English residuals).

Settled while translating the network-drive image-indexing feature (`settings.mediaIndex.networkVolumes.*` +
`alwaysIndex*` internals in `settings.json`, `search.imageResults.networkOff`/`.paused` in `search.json`, 2026-07-13):

- **photo vs image — MIRROR the EN split: `fotó` (photo) / `kép` (image).** The EN copy deliberately alternates: the
  settings CARD and global toggle say "image" (already shipped `settings.mediaIndex.card` = `Képkeresés`,
  `enabled.label` = `Képek tartalmának indexelése`), while the per-network-drive user strings say "photos" (a NAS holds
  photos). So user-facing per-drive strings and status lines use `fotó` (`fotók`, `fotói`, `fotóarchívum`), and the
  feature/section-level and INTERNAL developer labels keep `kép`/`képindexelés` (image indexing). `fotó` is Tier-1
  attested (macOS Photos app = `Fotók`); `kép` is settled (macOS AppKit "Image" = `Kép`). EN itself mixes the two even
  within one sentence (`search.imageResults.networkOff` = "image indexing … its photos"), so mirroring is faithful, not
  drift · high.
- network drive → `hálózati meghajtó` · macOS (`Hálózati meghajtó`, Tier 1) + settled `drive → meghajtó` · high.
- image indexing (the feature, internal labels) → `képindexelés` (transparent compound `kép`+`indexelés`, parallel to
  `fájlindexelés`); "Network drive image indexing" = `Hálózati meghajtók képindexelése` · high.
- "always index" (index a rarely-browsed drive regardless of browsing) → the impersonal subjunctive
  `mindig legyen indexelve` for the switch label/aria (`alwaysLabel` = `Ez a meghajtó mindig legyen indexelve`,
  `alwaysAria` = `{name} fotói mindig legyenek indexelve`), and `folyamatos indexelés` (continuous indexing) for the
  internal developer descriptions. `mindig` + a bare verbal noun is ungrammatical, so `legyen`/`legyenek` (3rd-person
  subjunctive, NOT `te` direct address, standard HU settings phrasing) carries "always/regardless" · tentative (no
  direct pile hit for the "index regardless of browse frequency" sense; construction reuses settled
  `index → indexelés` + `indexelve` state participle).
- **`{name}` (uncontrolled drive name) placed as a bare NOMINATIVE POSSESSOR, never suffixed**: `optInLabel` "Index
  photos on {name}" = `{name} fotóinak indexelése` ("{name}'s photos' indexing"), `alwaysAria` =
  `{name} fotói mindig legyenek indexelve`. Hungarian allows a juxtaposed nominative possessor (`naspi fotói` = "naspi's
  photos"), so the possessive suffix lands on `fotó`, not on the unknown `{name}` — dodges the vowel-harmony trap
  (style.md § Notes) the same way the colon-slot trick does, but reads more naturally for a possessive relation · high.
- disconnect / reconnect (a network drive) → disconnected STATE = `nincs csatlakoztatva` (is not connected;
  `search.imageResults.paused`); the drive disconnecting (event, intro) = `megszakad a kapcsolat a meghajtóval` (the
  connection to the drive breaks); reconnect = `újra csatlakozik` (`settings…paused`) · macOS (`újracsatlakoztat`
  attested; `Kapcsolat bontása` = disconnect) · high. Chose the natural `nincs csatlakoztatva`/`megszakad a kapcsolat`
  over a literal `leválasztva` because the drive drops on its own here (not a user-initiated eject/leválasztás).
- pause status reuses settled `pause → Szüneteltetve`; resume reuses settled `resume → Folytatás` (verb `folytatódik`,
  intransitive "it resumes"). "Indexing photos now" (live status) = `Fotók indexelése folyamatban` (matches the
  `queue.row.status` live-status `folyamatban` style + shipped `search.imageResults.indexing` "…folyamatban van"). "Not
  indexed yet" = `Még nincs indexelve`; the `indexed` count ICU keeps the singular noun in both branches
  (`{countText} fotó indexelve`, no-pluralize-after-number rule; `indexelve` = state participle) · high.
- "gently" (reads photos over the network sparingly) → `kíméletesen` (resource-sparing sense, fits the respect-resources
  tone) · descriptive · high. "at a limited speed" = `korlátozott sebességgel`, "only while you''re not busy" =
  `csak amikor épp nem vagy elfoglalt` (informal `te`, conversational copy).
- No `sameAsSourceJustification` needed: all 19 values differ from English.

Settled in the QUALITY-REVIEW pass over the 54 bulk-rename / image-index-scope / Ask-Cmdr-tool keys (`askCmdr.json`,
`errors.json`, `fileExplorer.json`, `settings.json`, 2026-07-20). These keys were first translated mid-feature without
the process; this pass re-derived every term from the pile:

- allow / deny (per-row approval buttons in the rename review) → `Engedélyezés` / `Elutasítás` · macOS Tier-1 directly
  (AppKit alert button `Engedélyezés`, `Engedélyezés mindenképp`, and `Elutasítás`/`Elutasítva`/`Kérés elutasítva`), MS
  (`allow` = `engedélyez`, `deny` = `elutasít`/`megtagad`) · high. "Allow all" / "Deny all" = `Összes engedélyezése` /
  `Összes elutasítása`, reusing the settled `Összes kijelölése`/`Összes átugrása` pattern.
- review (look over a set of proposed changes before applying) → `áttekintés` · macOS Finder/AppKit
  (`Módosítások áttekintése…` = "Review Changes…", `Nem mentett áttekintése`) · high. NOT MS's `vélemény`, which is the
  product-review sense (Microsoft wrong-sense trap 4). "Review file renames" = `Átnevezések áttekintése`; "This review
  expired" = `Ez az áttekintés lejárt` (`expire → lejár`, MS · high).
- **excluded (a folder the user took out of image indexing) → `kizár` / `kizárva`, NOT `kihagy`** · MS (`stop list` =
  `kizárási lista`, which IS the search-index exclusion sense; `evict` = `kizár`) AND, decisively, in-catalog
  consistency: the already-shipped `settings.mediaIndex.excludedFolders.label` = `Kizárt mappák` and its description
  ("…amelyeket a felhasználó **kizárt** a képindexelésből") name the SAME feature, and `queryUi.scope.hint` uses
  `kizáráshoz` · high. `kihagy`/`kihagyva` is already taken by the transfer `skipped` outcome (macOS
  `Kihagyás`/`Kihagyva`), so reusing it here blurred the deliberate-exclusion vs not-picked distinction the status-bar
  labels exist to make. Fixed `fileExplorer.imageIndex.excluded` → `Képek kizárva` and `.excludedTooltip` →
  `Kizártad ezt a mappát…`.
- blocked (a rename row preflight won't let through) → `blokkolva` · tentative. macOS attests `letiltva`, but that
  renders the policy/permission "disabled by Screen Time" sense, not "prevented from proceeding"; `blokkolva` is the
  common HU software word for the latter and is not an error word (the no-`hiba`/`sikertelen` voice rule holds). Used in
  `overwriteTooltip` and the SR summary `status` (`{blocked} blokkolva`).
- rename cycle → `átnevezési ciklus`; "rotating these files" → `a fájlok körbeforgatása`; "one temporary name" →
  `egy ideiglenes név` · MS (`cycle` = `ciklus`, `rotate` = `forgat`, `temporary file` = `ideiglenes fájl`) · high.
- **`cancel` in this dialog is `Mégsem`, never `Mégse`.** The pass found the catalog's single `Mégse` outlier
  (`askCmdr.renameReview.cancel`) against 15 `Mégsem`; macOS ships `Mégsem` 52× and `Mégse` 0×, and `style.md` already
  settled it. TC/DC say `Mégse` (42×), which is the Windows/Linux side of the split — not ours.
- **Brand suffix takes NO hyphen: `Cmdrt`, not `Cmdr-t`.** `Cmdr` is pronounced "commander", so its last written letter
  (`r`) does spell the last pronounced sound; AkH's hyphen rule (silent final letter / unusual letter cluster) doesn't
  apply. The catalog majority agrees (`Cmdrt` 11×, `Cmdrnek` 19×, `Cmdrben` 8×, `Cmdrrel`, `Cmdren`). Fixed
  `askCmdr.renameReview.expired` (`Ask Cmdr-t` → `Ask Cmdrt`). NOTE: three `licensing.json` values still write
  `a Cmdr-t`; out of scope for this pass, flagged for the next `licensing.json` touch.
- **`askCmdr.tool.*` done labels are `-va/-ve` PARTICIPLES, no exceptions.** `proposeRenamePlan.done` had drifted to
  `Átnevezési terv elkészült` (a finite past verb saying the plan "got finished"), breaking the family pattern every
  sibling follows (`listázva`, `ellenőrizve`, `megtalálva`, `átvizsgálva`, `használva`) and the doing/done parallelism
  the tool rail relies on. Fixed to `Átnevezési terv előkészítve`, pairing with `Átnevezési terv előkészítése`.
- `searchPhotos.doing`/`.done` → `A fotóid átvizsgálása` / `A fotóid átvizsgálva` KEPT as-is: even though the settled
  term is `search → keresés`, the shipped sibling `operationsList` renders the identical English frame ("Searching your
  X") as `A(z) X átvizsgálása`, and the `X keresése` pattern would read as "searching FOR your photos" rather than
  "searching among them". In-family consistency wins; recorded so it isn't relitigated.
- "next pass" (the next image-indexing sweep) → `a következő átvizsgálás` · reuses the settled `scan → átvizsgálás`,
  which is the whole `driveIndex` family's term (`Újbóli átvizsgálás`, `Vizsgáld át újra`) · high. Replaced the vaguer
  `a következő kör` in `imageIndex.indexedTooltip`.
- "work out which ones matter to you" → `állapítsa meg, melyek fontosak neked` (not `találja ki`, which reads as
  "guess") · descriptive; `importance → fontosság` is already settled (`folderImportance` tool key) · high.
- `{percent}` may safely take `-nál` (`{percent}%-nál tart`): the suffix attaches to the `%` glyph, read `százaléknál`,
  which is invariably back-vowel — the same constrained-domain reasoning as the FAT32 `{maxSize}-nál` entry · high.
- No `sameAsSourceJustification` needed in this batch: all 54 values differ from English.

Settled while translating the image-index indicator badges (`fileExplorer.imageIndex.file/folder/drive.*` +
`settings.mediaIndex.showFileStatusIcons.*`, 13 keys, 2026-07-22):

- **badge (a small status overlay on a file/drive icon) → `jelvény`; status badge → `állapotjelvény`** · MS terminology
  (`notification badge` = `értesítési jelvény`, `badge` = `jelvény`) AND macOS Finder Tier-1 concept parallel: Finder
  overlays iCloud-sync status badges on file icons with the `BADGE_AX_LABEL` container + `AXBADGE*` state aria labels
  (the exact same UI element Cmdr builds here) · high. status = `állapot` (settled, `driveIndex.ariaLabel` =
  `indexállapot`). Settings label "Show status badges on image files" = `Állapotjelvények megjelenítése a képfájlokon`
  (nominal; `képfájl` = image file, compound `kép`+`fájl`; "small badge" in the description = `kis jelvény`).
- **badge STATE labels follow Finder's file-icon badge pattern: done = `-va/-ve` participle, waiting =
  `Várakozás a(z) X-re`** · macOS Finder Tier-1 directly (`AXBADGE0` "Downloaded" = `Letöltve`, `AXBADGE4/5` "Waiting to
  upload/download" = `Várakozás a feltöltésre`/`a letöltésre`) · high. So `file.indexed` "Indexed for image search" =
  `Indexelve a képkereséshez` (`indexelve` state participle, settled; `képkeresés` allative `-hez`); `file.pending`
  "Waiting to be indexed" = `Várakozás az indexelésre` (the Finder `Várakozás a(z) X-re` frame, `az` before the
  vowel-initial `indexelés`). `file.stale` "Changed since indexing; will be re-indexed" =
  `Megváltozott az indexelés óta; újra lesz indexelve` (`óta` postposition dodges suffixing; `újra lesz indexelve` =
  future state).
- **`file.failed` "Couldn''t be indexed" → `Nem sikerült indexelni`, NOT Finder''s `Hiba`** · the settled `nem sikerült`
  calm-voice rule + the no-bare-"hiba"/"sikertelen" voice rule; the EN itself softened "failed" to "couldn''t", and
  Finder''s badge `AXBADGE1` = `Hiba` is deliberately NOT followed here · high.
- **`file.excluded` "Not included in image search" → `Nem szerepel a képkeresésben`, NOT `kizárva`** · the EN word is
  the neutral "not included" (the `@key` lists several non-user reasons: out of scope, unsupported, too big), so the
  deliberate-user-exclusion term `kizár`/`kizárva` (settled for `settings.mediaIndex.excludedFolders` = `Kizárt mappák`)
  would over-claim. `szerepel` = to appear/be included; `képkeresés` inessive `-ben` (front) · high. Distinct on purpose
  from the folder-level `imageIndex.excluded` (deliberate exclusion) which stays `kizárva`.
- **image search (feature) → `Képkeresés`** · REUSED verbatim from the already-shipped `settings.mediaIndex.card` /
  `settings.section.imageSearch` = `Képkeresés` · high. Don''t fork it.
- **drive image-search dot aria (`drive.ariaLabel`) → `Meghajtó képkeresési állapota`** · parallels the sibling
  index-status dot `fileExplorer.navigation.driveIndex.ariaLabel` = `Meghajtó indexállapota` (same nominal, no-article,
  no-"this" shape; the two dots sit adjacent) · high. `drive.off` "Image search is off for this drive." =
  `A képkeresés ki van kapcsolva ehhez a meghajtóhoz.` reuses the shipped `driveIndex.tooltipDisabled` frame
  (`Az indexelés ki van kapcsolva ehhez a meghajtóhoz.`) verbatim, swapping the subject · high.
- **count-of-count fragments use the `x / y` slash idiom, never a suffixed count placeholder** · the catalog''s settled
  slash idiom (`viewer` match position `{current} / {total}`) dodges the vowel-harmony trap that a `{totalText}`-ból
  elative "of" would hit. `folder.someIndexed` = `{doneText} / {totalText} {plural kép} indexelve`; `folder.allIndexed`
  "All N images indexed" = `Mind a(z) {totalText} {plural kép} indexelve` (`Mind a/az` + numeral + SINGULAR counted
  noun, no-pluralize-after-number rule → both plural branches are `kép`; `a(z)` for the unknown numeral''s article).
  Terse fragments drop `van` (mirroring EN''s verbless "indexed"); the full-sentence `drive.done` "All N … are indexed."
  KEEPS it (mirroring EN''s "are"): `Ezen a meghajtón mind a(z) {totalText} {plural kép} indexelve van.`
  `drive.indexing` reuses the live-progress `folyamatban van` (shipped `search.imageResults.indexing`):
  `{doneText} / {totalText} {plural kép} indexelve ezen a meghajtón; még folyamatban van.` · high.
- No `sameAsSourceJustification` needed in this batch: all 13 values differ from English.

Settled while translating the image-indexing settings restructure + "Indexing now" badge (12 keys, `settings.json` +
`fileExplorer.json`, 2026-07-22):

- **"search by description" (semantic-search feature concept) → `leírás szerinti keresés`** (attributive) /
  `leírás alapján` (adverbial) · REUSED from the shipped `settings.mediaIndex.clip.ready` =
  `… keress a fotóid között leírás alapján` and `clip.description` (`… a tartalmuk leírásával …`) · high. The nominal
  subject/object form uses the clean attributive `leírás szerinti keresés` (`settings.mediaIndex.clip.offButInstalled`,
  `…deleteConfirmBody`, `…notSupported`); the toggle LABEL mirrors the shipped adverbial phrasing: "Search photos by
  description" = `Fotók keresése leírás alapján` (`settings.mediaIndex.semanticSearch.label`,
  nominal/no-direct-address). Don''t fork into `leírás alapú` or a verb form.
- **Apple silicon → kept verbatim `Apple silicon`** · en `@key` "keep it"; no reference-pile localization exists (grep
  of `_ignored/i18n/hu/` found zero `Apple silicon`/`Apple chip`/`szilícium` hits) · high. Connector uses the
  pile-dominant `chip` (46 hits vs 13 `lapka`): "needs a Mac with Apple silicon" =
  `Apple silicon chippel szerelt Mac szükséges` (`settings.mediaIndex.clip.notSupported`). Phrased so the kept brand
  token takes no Hungarian case suffix.
- **card titles (image-indexing settings restructure)**: "Enable indexing" = `Indexelés bekapcsolása`
  (`enable → bekapcsolás`, matching `settings.network.enabled.label` = `… bekapcsolása`); "Folders to index" =
  `Indexelendő mappák` (`-endő` "to-be-…" gerundive, terse card title); nominal/no-direct-address per label style ·
  high.
- **"Indexing now" → `Indexelés folyamatban`** · used for BOTH surfaces sharing the English string + `sourceHash`
  (`settings.mediaIndex.progressSummary.title` heading and `fileExplorer.imageIndex.file.indexing` badge tooltip), kept
  consistent. `folyamatban` ("in progress") carries the "now"; reuses the catalog''s `folyamatban van` live-progress
  idiom (shipped `search.imageResults.indexing`). Contrasts the sibling badge states `.indexed` =
  `Indexelve a képkereséshez` (done) and `.pending` = `Várakozás az indexelésre` (queued) · high.
- **delete-model flow (reuses shipped `clip.download`/`clip.downloading` parallels)**: "Delete model (reclaim {size})" =
  `Modell törlése ({size} felszabadítása)` (mirrors `clip.download` = `Modell letöltése (~{sizeText} MB)`;
  `reclaim → felszabadítás`, pile-attested + shipped `reclaim.button`/`reclaim.freed`); "Deleting…" = `Törlés…` (mirrors
  `clip.downloading` = `Letöltés…`); confirm title (a question, informal `te`) "Delete the semantic search model?" =
  `Törlöd a szemantikus keresés modelljét?` (reuses `clip.title` = `Szemantikus keresés`); confirm body avoids suffixing
  the `{size}` placeholder via the intransitive `Ezzel felszabadul {size}, és kikapcsol a leírás szerinti keresés. …`
  ("keyword search" = `kulcsszavas keresés`, pile `kulcsszó`; "tag search" = `címke szerinti keresés`, `tag → címke`);
  delete-failure stays calm/no-"hiba" per the voice rule: "The model couldn''t be removed just now. Try again in a
  moment." = `A modellt most nem sikerült eltávolítani. Próbáld újra egy pillanat múlva.` (`nem sikerült` calm rule +
  informal `próbáld újra`) · high.
- No `sameAsSourceJustification` needed: all 12 values differ from English.

Settled while translating the delete-switch and transfer From/To keys (`fileOperations.json`, 2026-07-23): the delete
dialog swapped its Kuka/Törlés picker for a "Move to trash" switch plus a matching confirm button, and the
copy/move/compress dialog groups the source path and the destination volume+path under "From" and "To" headings.

- "Move to trash" (`delete.trashSwitch`; switch in the delete dialog, on = Kuka, off = permanent delete) →
  `Áthelyezés a Kukába` · already settled catalog-wide (`transferDialog.titleVerbOnly` `other` arm, `queue.row.label`
  trash arm, `operationLog.summary` trash arm); macOS Finder AL13/N153 `Áthelyezés a kukába` confirms the phrase, and
  the catalog keeps `Kuka` capitalized as the feature name (settled) · high
- "Delete" (`delete.confirmDelete`; destructive confirm button while the switch is off) → `Törlés` · settled delete
  noun, identical to `transferDialog.titleVerbOnly`'s `delete {Törlés}` arm · high
- "From" / "To" (`transferDialog.sourceGroupTitle` / `targetGroupTitle`; headings over the source path and over the
  destination volume + path) → `Forrás` / `Cél` · Total Commander hu (`662="Forrás:  "`, `663="Cél   : "`) and Double
  Commander hu ("Forrás:"/"Cél:") both ship this label pair in the same copy/move dialog, and `Cél` is what the group's
  own controls already carry (`destVolumeAria` = `Célkötet`, `destPathAria` = `Célútvonal`), so heading and contents
  agree. Hungarian has no standalone from/to prepositions: the deictic `Innen` / `Ide` is what macOS uses INSIDE a verb
  phrase (`Move To:` = `Áthelyezés ide:`) and dangles as a bare heading, so the source/target nouns win here · high

Settled in the REVIEW pass over the five drive-indexing-override keys
(`fileExplorer.navigation.driveIndex.*IndexingOff*`

- `settings.indexing.masterOffNote`/`.overriddenBadge`, 2026-07-27):

* **"drive indexing" (the feature/master switch) → `(a) meghajtó indexelése`, NEVER the compound `meghajtó-indexelés`**
  · in-catalog frequency is decisive: the possessive phrase already ships 6× as the concept's name
  (`settings.indexing.enabled.label`, `settings.section.driveIndexing`, `settings.summary.driveIndexing`,
  `onboarding.stepOptional.indexing.title`, and in running prose `onboarding…descIntro` = "A meghajtó indexelése
  kifejezetten klassz!", plus `reEnableNotifications.description` = "…minden meghajtó indexelésére"), while
  `meghajtó-indexelés` had exactly 4 hits, all of them these new keys · high. The hyphen is also unsourced: Microsoft
  terminology writes two-part index compounds SOLID (`indexfájl`, `listaindex`, `színindex`, `indexpartíció`,
  `tartalomindexelés`, `mélyindexelési`), hyphenating only proper-name-headed ones (`Csomagerőforrás-indexelő`). So if a
  compound is ever wanted it is `meghajtóindexelés`, not `meghajtó-indexelés`.
  - Register split, both correct and already settled: the FULL form `(a) meghajtó indexelése` names the global
    feature/switch (and is what a `Indexelés > Meghajtó indexelése` navigation path must quote verbatim); the bare
    `indexelés` is the anaphoric short form inside per-drive controls and badges (`driveIndex.tooltipDisabled` = "Az
    indexelés ki van kapcsolva ehhez a meghajtóhoz.", `driveIndex.menuEnable`, `indexing.firstConnect.enable` =
    "Indexelés bekapcsolása"). The noun "drive index" (the data) stays the solid `meghajtóindex`
    (`queryUi.results.indexUnavailable`) or the possessive `a meghajtó indexe`.
* **"stays unindexed" → `továbbra sem lesz indexelve`, not the coined adjective `indexeletlen`** · Total Commander hu
  ships the state participle directly (`2050="A mappa mérete nincs indexelve…"`), and the catalog already uses it
  (`fileExplorer.imageIndex.*` = `Még nincs indexelve`, `Indexelve`). `indexeletlen` had a single catalog hit and no
  pile support · high.
* **"picks up where it left off" → `ott folytatja majd, ahol abbahagyta`** · `ott … ahol` is the correct Hungarian
  correlative pair for this idiom (`onnan` pairs with `ahonnan`, so `onnan … ahol` is ungrammatical); `majd` carries the
  English future sense of "turn it on, and it picks up" once the clause stands on its own · high.
* **`settings.indexing.overriddenBadge` "Off with drive indexing" → `Az indexeléssel együtt ki`** (kept) · badge brevity
  is a hard constraint (25 chars vs the English 23), so it uses the settled anaphoric short form `indexelés` plus
  `együtt`, which forces the comitative "along with" reading instead of the instrumental "by means of" one that a bare
  `Az indexeléssel kikapcsolva` would allow. `Ki` as an off-state word is catalog-settled
  (`settings.ai.provider.opt.off` = `Ki`) · high.

## Meghajtóindex: a változásellenőrző futás (2026-07-28)

- **"Checking for changes" (run-kind header) → `Változások ellenőrzése`** · deverbal-noun phrase matching the sibling
  headers (`Első teljes átvizsgálás`, `Gyors frissítés`); `ellenőrzése` is macOS HU's checking noun (Finder BN9 „^0”
  tartalmának ellenőrzése), `változások` is catalog-settled (`Legutóbbi változások pótlása`) · high.
- **"Update the file list" → `Fájllista frissítése`** · composed from the settled siblings `Fájllista mentése` +
  `Index frissítése` · high.
- **"the check running right now" → `az éppen futó átvizsgálás`** · reuses `átvizsgálás` as this catalog's settled word
  for a full check (`tooltipCoalesced`: "a Cmdr következő teljes átvizsgálása") and that string's closing
  `ezt rendbe hozza` · high.

Settled while translating the stalled-transfer notice (7 `fileOperations.transferProgress.stall*`/`close` keys +
`queue.row.stalled`, 2026-07-31):

- **"No progress for {duration}" → `{duration} óta nincs előrehaladás`** · `előrehaladás` is the pile's progress noun
  (macOS Finder `1.title` = `Előrehaladás paraméterei`, Xfce Thunar "File operation progress" =
  `Fájlművelet előrehaladása`) and is already what this catalog calls it (`sizeProgressAria` = `Méret-előrehaladás`,
  `fileProgressAria` = `Fájl-előrehaladás`) · high. The `{placeholder} óta` shape is pile-attested (Nautilus "Since %s"
  = `%s óta`) and keeps the placeholder UNSUFFIXED, which the style guide requires: `{duration}` renders as the
  un-localized `45s` / `2m 30s` / `1h 5m` (`$lib/units/duration.ts` formats digits + Latin unit letters, no locale
  branch), so no `-e`/`-ja` adverbial suffix could vowel-harmonize with it. Duration-first word order also mirrors the
  line it replaces (`etaRemaining` = `~{duration} van hátra`). Residual `tentative` point: `óta` most often takes a
  point in time; with a measured span it's idiomatic in the plural (`hetek óta`) and reads fine with an abbreviated
  value, but a native reviewer may prefer `{duration} alatt nem történt előrehaladás` (unambiguously a span, longer, and
  slightly past-tense). Both keys use the identical sentence, only the dialog one takes the period, as in English.
- **"Waiting for X to respond" → `Várakozás a X válaszára`** (destination = `Várakozás a cél válaszára.`, source =
  `Várakozás a forrás válaszára.`) · Total Commander `1384="Adatküldés, várakozás a válaszra..."` is the exact
  waiting-for-a-response phrase, and the `Várakozás a …-ra/-re` frame is macOS Tier 1 (AppKit "Waiting for disc drive…"
  = `Várakozás a lemezmeghajtóra…`; Finder `Várakozás a feltöltésre`, `Várakozás a letöltésre`,
  `Várakozás „^0” általi fogadásra…`), plus Double Commander (`Várakozás a fájlforrás elérésére`,
  `Várakozás felhasználói válaszra`) · high. The two sides are named with the dialog's OWN group headings `Cél` /
  `Forrás` (settled 2026-07-23 from TC `662/663` and DC), so the notice and the boxes it explains use one word each;
  don't fork to `célhely`/`céleszköz` here. `nem reagál` (macOS AppKit's "did not respond") is the negative,
  fault-flavored form and is deliberately NOT used: the notice states what Cmdr is doing (waiting), not that something
  is broken.
- **"The transfer has stopped moving" → `Az átvitel megállt`** · no source names a stalled-but-alive transfer (mining
  gotcha 3's shape: the concept is absent from every corpus; macOS HU has zero `megállt`/`leállt` hits, Microsoft
  terminology has no `stall`/`stalled`/`unresponsive` entry), so this is composed from settled `transfer → átvitel` plus
  the plain intransitive `megáll` · tentative. NOT `leállt` (reads as "shut down / ended", and the transfer is still
  alive), NOT MS's `leállítás` (that's the deliberate "stop" command), and NOT a second `nincs előrehaladás`, which
  would just repeat the line above it.
- **"Cancel it, or leave it running in the background." → `Szakítsd meg, vagy hagyd futni a háttérben.`** · reuses the
  settled running-op `cancel → megszakítás` (imperative `szakítsd meg`, informal `te` per Formality, as in the settled
  `próbáld újra`) and the settled `Hagyd futni a háttérben` (from `queueTooltip`) verbatim · high. Comma before the
  clause-joining `vagy` is correct and pile-attested (Nautilus/Dolphin:
  `Nevezze át a szimbolikus hivatkozást, vagy nyomja meg a Kihagyás gombot.`).
- **"{N} file(s) is/are still open" → `{count, plural, one {# fájl van még nyitva} other {# fájl van még nyitva}}`** ·
  Total Commander `616="Túl sok fájl van nyitva."` gives both the term and the `… fájl van nyitva` word order; `még`
  carries the English "still" · high. Both branches identical (Hungarian no-pluralize-after-a-numeral rule, as in
  `queuedToastCount`/`selectedCount`), and the counted noun keeps the singular verb, so the trailing clause stays
  singular too.
- **"and may already be partly written" → `és lehet, hogy már részben ki van írva`** · `kiír` is this catalog's verb for
  writing bytes out (`transferProgress.titleFlushing` = `Az utolsó darab kiírása…`), and the stative `ki van írva`
  avoids both the bureaucratic `kiírásra került` and a `-tuk/-tük` first-person that would put words in Cmdr's mouth ·
  high. Kept OUTSIDE the plural braces, as in English.
- **"The log has the details." → `A részleteket megtalálod a naplóban.`** · reuses settled `log → napló` (`naplófájl`,
  `Naplózás`; TC `5390="Napló fájl"`, DC `Naplófájl megtekintése`) and the catalog's own "you'll find it there" shape
  (`backgroundedToast` = `Megtalálod az átviteli sorban.`), which reads warmer than the literal
  `A napló tartalmazza a részleteket.` · high.
- **`transferProgress.close` (dismiss the dialog while the transfer finishes) → `Bezárás`** · the catalog-wide,
  macOS-sourced Close (`ui.modalDialog.close`, `fileOperations.errorDialog.close`, and 8 more) · high. It sits next to
  `fileOperations.button.cancel` = `Mégsem`, so the pair is unambiguous: `Bezárás` closes the window, `Mégsem` stops the
  operation.
- No `sameAsSourceJustification` needed in this batch: all 8 values differ from English.

## Másolt útvonal: a vágólap-visszajelzés (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

Egy kulcs: a ⌃⌘C utáni információs toast szövege. Maga az útvonal alatta, külön, fix szélességű sorban jelenik meg,
tehát NEM helyőrző a mondatban: a mondat kettősponttal zárul, és önmagában is állnia kell.

- **"Copied the path, it's now on your clipboard:" → `Útvonal másolva, most már a vágólapon van:`** · a bevett
  `clipboard → vágólap` és `path → útvonal` (`Ugrás útvonalra`) szótári döntéseket használja · high. Az `-va/-ve`
  határozói igenév a testvér toastok mintája (`{countText} elem másolva`). Birtokos rag nélkül (`a vágólapon`, nem
  `a vágólapodon`): egy vágólap van, a macOS is névelővel mondja.
- `sameAsSourceJustification` nem kell: az érték eltér az angoltól.

Settled while re-translating the renamed queue window (14 keys in `queue.json`, `commands.json`, `fileOperations.json`,
2026-08-08). English widened the window's name from "Transfer queue" to **"Operation queue"**: the window lists deletes,
trashes, renames, folder/file creations, and archive edits too, and "transfer" already means copy-or-move one level down
(the transfer dialog, the transfer driver). So the Hungarian head noun had to widen the same way, not just get
restamped:

- **operation queue (the window, the View-menu item, the command) → `Műveleti sor`** · SUPERSEDES the June
  `transfer queue → átviteli sor` entry (see the 2026-06-21 transfer-queue block above), which stays on record because
  `transfer → átvitel` itself is unchanged and still correct for the copy/move dialog · high. Built from two settled
  parts: the head noun `művelet` (below) and the catalog's settled `queue → sor` (Double Commander `New queue` =
  `Új sor`, `Put first in queue` = `Első helyre tétele a sorban`). The `<activity>-i sor` shape is Tier-2 attested
  (Microsoft `print queue` = `nyomtatási sor`) and is the exact shape the outgoing `Átviteli sor` used, so only the
  modifier changes; the adjectival `műveleti` + head-noun formation is Double-Commander-attested (`operations panel` =
  `műveleti panel`).
  - **NOT the solid compound `Műveletsor`**, even though it would look more parallel to `Műveletnapló`: Microsoft
    terminology already assigns `műveletsor` to `task flow` (id 2335491) and `visszaállítási műveletsor` to
    `restore sequence` (id 2225865) — that is, a SEQUENCE of steps, not a waiting line. The compound would name the
    wrong concept.
  - NOT Microsoft's generic `queue` = `várakozási sor` either: the catalog settled the file-manager-native `sor` in June
    and `várakozási sor` is long for a window title.
  - Inflects regularly (back-vowel `sor`): illative `a műveleti sorba` (`transferProgress.queueAria`), inessive
    `a műveleti sorban` (`queueTooltip`, `queuedToast`, `backgroundedToast`). **Watch the article**: `Átviteli` starts
    with a vowel and took `az`, `műveleti` starts with a consonant and takes `a` — every one of those sites moved from
    `az átviteli sorba/-ban` to `a műveleti sorba/-ban`.
- **operation (the category word: a copy, move, delete, trash, rename, folder/file creation, or archive edit) →
  `művelet`** · macOS Tier 1 throughout (`Művelet` as a bare label; "A művelet nem hajtható végre.", "Ez a művelet nem
  vonható vissza.", `Gyorsműveletek`), Microsoft terminology (`operation` = `művelet`, two entries), Double Commander
  (`Current operation:` = `Aktuális művelet:`, `Executing operations` = `Műveletek végrehajtása`, `File operations` =
  `Fájlműveletek`) · high. **Matches the shipped Operation log window** (`Műveletnapló`, settled 2026-07-09) and the
  settled `action/operation → művelet` (`Fájlműveletek`), so the deliberate English View-menu pair "Operation queue" /
  "Operation log" survives as `Műveleti sor` / `Műveletnapló`. Do NOT fork the head noun: two different words in two
  neighbouring menu items would be the defect. Inflects front-vowel: dative `a műveletnek`, accusative `a műveletet`,
  plural `Műveletek`.
- Row screen-reader labels keep their nominal shape and only swap the noun: `Ennek a műveletnek a szüneteltetése` /
  `… a folytatása` / `… a megszakítása` / `… a kijelölése` (was `Ennek az átvitelnek a …`). The heading and the list
  aria (`Operations`) are the bare plural `Műveletek`.
- `commands.queueShow.label` is now the bare window title `Műveleti sor` (English dropped "Show"), matching the sibling
  `commands.logOperationLog.label` = `Műveletnapló`, which is also bare.
- Counted-noun plural keeps the singular in both branches, as always: `queuedToastCount` =
  `{count, plural, one {# művelet} other {# művelet}}` (was `{# átvitel}`).
- Untouched on purpose: `queue.empty.title` = `A sor üres` (bare anaphoric `sor`, still correct) and
  `transferProgress.titleCancellingSlow` = `Megszakítás… (USB-átvitelek befejezése)` (a real transfer, not the queue).
- No `sameAsSourceJustification` needed: all 14 values differ from English.

Settled while translating the corner progress chip and the failure notice (9 keys in `queue.json`, 2026-08-08). Two new
surfaces: a ~80 px chip in the main window's top-right corner previewing the operation running in the background (a
verb, a bar, a hover tooltip, and a stopped-early state), and a never-auto-dismissing toast for an operation that
couldn't finish, with a matching row + Dismiss button in the queue window. The window's name, the head noun `művelet`,
and their inflection are settled in the rename block directly above; these keys only reuse them.

- **dismiss (stop showing a notice or a finished-with row) → `Elvetés`** (button), `Mindet elveti` (toolbar),
  `Ennek a műveletnek az elvetése` (row aria) · the shipped `hu` catalog is decisive: `Elvetés` already renders every
  `Dismiss` in the app (`crashReporter.dialog.dismiss`, `lowDiskSpace.toast.closeTooltip`, `downloads.empty.dismiss`,
  `downloads.fda.dismiss`, `errorReporter.sentToast.dismiss`, `errorReporter.bundleSavedToast.dismiss`,
  `fileOperations.mkdir.timeoutDismiss`), and `ui.toast.dismissAria` = `Értesítés elvetése` is this exact
  dismiss-a-notification sense · high (upgraded from the `fileOperations`-pass `tentative`; seven shipped sites plus a
  matching aria is stronger evidence than the pile). Deliberately NOT the pile term: macOS has one hit only (AppKit
  TouchBar "Dismiss Popover" = `Előugró üzenet bezárása`) and Microsoft terminology gives `bezárás` / `leállítás` for
  the notification sense, but `Bezárás` is the catalog's settled `Close` (`ui.modalDialog.close` and 10 more), so
  adopting it would make the queue row's Dismiss indistinguishable from a window's Close.
  - `Mindet elveti` takes the finite-verb shape its two neighbours settled (`Mindet szünetelteti`, `Mindet folytatja`),
    not the nominal shape of `Kijelöltek megszakítása`; English's "Dismiss all" is parallel to "Pause all"/"Resume all",
    so the Hungarian is too.
  - The row aria joins the `Ennek a műveletnek a …` family (`… a szüneteltetése` / `… a folytatása` / `… a megszakítása`
    / `… a kijelölése`) and is the only member taking `az`: `elvetése` is vowel-initial.
- **"couldn't finish <action>" (the failure toast's nine `select` arms) → `Nem sikerült befejezni` + the action in the
  accusative** · built from the `queue.row.status` `failed` arm (`Nem sikerült befejezni`) so the toast and the queue
  row can't word the same event differently, with each action's noun taken verbatim from `queue.row.label` /
  `operationLog.summary.*` and put in the accusative: `a másolást`, `az áthelyezést`, `a törlést`,
  `az áthelyezést a Kukába`, `az átnevezést`, `a mappa létrehozását`, `a fájl létrehozását`,
  `az archívum szerkesztését`; the `other` arm is the bare row wording · high. Verb-first in every arm (not the
  topic-first `A másolást nem sikerült befejezni`) so the nine read as one family and the `other` arm is a clean
  truncation of the rest, exactly as in English. The `nem sikerült` calm-voice rule keeps `hiba` / `sikertelen` out.
- **"{n} operations couldn't finish" (summary toast + chip) → `{countText} műveletet nem sikerült befejezni`** · the
  same house wording with the count as its accusative object, which is how Hungarian expresses this impersonally · high.
  Counted noun stays SINGULAR in both plural branches (`műveletet`, never `műveleteket`), as in `queuedToastCount` /
  `selectedCount`.
- **"Show in operation queue" (failure-toast button) → `Megjelenítés a műveleti sorban`** · the catalog's own
  `Show in <place>` shape, shipped twice as `Megjelenítés a Finderben` (`commands.fileShowInFinder.mac.label`,
  `errorReporter.bundleSavedToast.reveal`); inessive `a műveleti sorban` per the rename block · high.
- **"Open the operation queue [to see why]" (the chip's promise, `chip.ariaLabel` + `chip.failed`) →
  `Nyisd meg a műveleti sort` / `Nyisd meg a műveleti sort, hogy megtudd, miért.`** · informal `te` imperative, matching
  the catalog's other second-person instructions (`próbáld újra`, `Szakítsd meg, vagy hagyd futni a háttérben.`,
  `Kattints ide az ugráshoz: {path}`); accusative `a műveleti sort` · high. The imperative is deliberate over a
  declarative "the reason is in the queue": the chip IS the button, so the sentence has to name the action pressing it
  performs.
- **"percent" spelled as a word (screen-reader label) → `százalék`, unsuffixed** · Hungarian screen readers expand `%`
  to `százalék` anyway, so spelling it out costs nothing and removes the dependency; `{percentText} százalék` needs no
  case suffix, which also keeps the placeholder unsuffixed (per `style.md` § Notes and decisions) · high. NOTE the split
  with the chip TOOLTIP, which is read by eye and keeps the glyph: Hungarian sets `%` tight against the number with NO
  space (`42%`), unlike de/fr/sv.
- **item (a file-or-folder in a count) → `elem`** · macOS Finder Tier 1 throughout (`Kuka elemei`,
  `Másolni kívánt elemek`, `Nincsenek törölni kívánt elemek.`) and already the catalog's counted-item noun
  (`fileExplorer.clipboard.copied` = `{countText} … elem másolva`, `operationLog.summary.*` = `{countText} elem …`,
  `operationLog.dialog.moreItems`) · high. Singular in both plural branches.

### `queue.chip.tooltip`: the dot-separated fact line

The hardest string in the batch. English continues a phrase before switching to middle dots
(`Copying 214 items to Backup · 42% · 1m 20s left`); Hungarian can't, because `{label}` arrives as a verbal NOUN
(`Másolás`, from `queue.row.label`), and `Másolás 214 elem` is ungrammatical. So **every optional clause carries its own
`·` inside its branch** and the whole line is one flat fact list:

`{label}{count, plural, =0 {} one { · {countText} elem} other { · {countText} elem}}{hasDestination, select, yes { · ide: {destination}} other {}} · {percentText}%{hasDetail, select, yes { · {detail}} other {}}`

- The flat shape is in-catalog precedent, not an invention: `ai.toast.progress` =
  `{percentText} · {downloaded} / {total} · {speed}/s{eta, select, none {} other { · {eta} van hátra}}` is the same
  Hungarian dot-separated progress line with the same optional-clause-carries-its-own-separator discipline.
- **destination → ` · ide: {destination}`**, never a case suffix on the folder name · macOS Tier 1 ships exactly this
  action-then-`ide:` shape (`Áthelyezés ide: %@` 78×, `„^0” letöltése ide: „^1”`, `Tömörítés ide: „^0”`,
  `… másolása és áthelyezése ide: ${destination}`), and the verb it wants is already at the head of the line · high. A
  runtime folder name can't take a harmonizing suffix (`Backupba`/`Backupbe`), which `style.md` forbids (§ Notes and
  decisions). Runner up was the settled transfer-dialog heading `Cél:`; `ide:` wins because it reads as a continuation
  of the leading verb instead of a form label.
- `{detail}` arrives ALREADY in Hungarian and must not be re-derived here: `OperationChip.svelte` fills it from
  `fileOperations.transferProgress.etaRemaining` (= `{duration} van hátra`, settled 2026-07-31) or from the
  `queue.row.status` `paused` arm (`Szüneteltetve`). So the time-left phrasing for this surface is inherited, not
  chosen.
- Assembled and read for all four combinations, all clean (no double space, no dangling `·`): `Másolás · 42%` ·
  `Másolás · ide: Backup · 42%` · `Másolás · 3 elem · 42%` · `Másolás · 3 elem · ide: Backup · 42%`, each optionally
  plus ` · 1m 20s van hátra`.
- The `=0 {}` and `other {}` empty arms stay empty, and `{label}` keeps NO leading separator: it is the only
  always-present part before `{percentText}`.

Flagged for a future reviewer: the failure toast's longest arm, `Nem sikerült befejezni az áthelyezést a Kukába` (46
chars vs English's 31), is the overflow risk in a ~360 px toast; `archive_edit` (48) is longer still but far rarer. If
either wraps badly, the fix is the shorter macOS variant `a Kukába helyezést` (attested alongside
`Áthelyezés a Kukába`), not a new failure wording.

No `sameAsSourceJustification` needed: all nine values differ from English.

Settled while translating the standalone conflict prompt (2 keys in `fileOperations.json`, 2026-08-09). The prompt is
hosted by the main window when a backgrounded operation hits a name clash, so a context line names which operation is
asking and a quiet note explains why the rest of the queue stopped.

- **`operationConflict.context`: the four `hasDestination: yes` arms keep the settled `queue.row.label` verbal nouns and
  add the destination as a deictic colon clause, never as a case suffix** · `Másolás ide: {destination}` /
  `Áthelyezés ide: {destination}` · macOS Finder Tier 1 (`Áthelyezés ide: %@`, plus the menu items `Másolás ide`,
  `Áthelyezés ide…`) and Nautilus (`Fájlok másolása ide: „%s”…`, `„%s” másolása ide: „%s”`) both ship this exact
  verb-then-`ide:` shape, and it is already the catalog's own rendering in `queue.chip.tooltip`
  (` · ide: {destination}`) · high. Unquoted, matching English and the chip tooltip; macOS quotes the name in this
  shape, but mixing quoted and unquoted arms inside one select would be the worse defect.
- **`ide:` (illative, "to") vs `itt:` (locative, "in") tracks English's own preposition split across the arms** ·
  copy/move say "to {destination}" so they take `ide:`; the `other` arm says "Working **in** {destination}" (work
  happening inside a folder, not items going into one) so it takes `Folyamatban itt: {destination}`, the catalog's
  settled `itt: {placeholder}` neutral slot (`errors.*` `nem található itt: {hostName}`,
  `Bármikor visszavonhatod itt: {systemSettings}`) on top of the sibling's `Folyamatban` · high.
- **An uncontrolled placeholder can sit in the POSSESSOR slot of a possessive verbal-noun phrase, which needs no suffix
  on it at all**: "Editing {destination}" (the archive itself) → `{destination} szerkesztése` · the possessor is
  unmarked in Hungarian, so only the head noun inflects (`-e`), and the pile ships the shape with a runtime name in that
  slot (macOS `„^0” másolása szüneteltetve lett`, Nautilus `„%s” másolása ide: „%s”`; Thunar/TC `Fájlnév szerkesztése`,
  `Eszközsor fájl szerkesztése`) · high. This is a third suffix-dodge alongside the postposition and the `itt:`/`ide:`
  colon slot already recorded in `style.md` § Notes and decisions, and the only one that keeps the value in subject
  position. No article, since `a`/`az` would have to agree with an unknown first sound. The generic `other`-branch arm
  stays the sibling's `Archívum szerkesztése` ("Editing an archive"), so the English yes/other distinction survives.
- **`operationConflict.pausedNote` "Everything else is paused until you answer." →
  `Minden más szüneteltetve van, amíg nem válaszolsz.`** · `szüneteltetve` is lifted verbatim from the
  `queue.row.status` `paused` arm so the prompt and the queue rows word one state with one word (macOS confirms it in
  running prose: `A(z) „^0” másolása szüneteltetve lett`); the trailing `amíg nem …` clause is macOS Tier 1
  (`Tartsa csatlakoztatva az eszközt, amíg a törlés be nem fejeződik.`) and needs no `addig` correlative; informal `te`
  (`válaszolsz`) per Formality; `minden más` is already the catalog's phrase (`zárj be minden más appot`) · high.
  Deliberately the stative `szüneteltetve van` over the intransitive `szünetel`: the latter is correct Hungarian but
  would show the user a different word than the row they are being told about. Settled while translating the empty-queue
  state of the progress dialog's primary button (`fileOperations.transferProgress.background` + `.backgroundAria`,
  2026-08-09). Same button as `.queue` = `Sorba`, worded for an EMPTY operation queue: with nothing to queue behind, it
  names what pressing it does instead:

- **"Background" (the button, a verb: put this transfer in the background) → `Háttérben`** · Total Commander hu is a
  direct hit on THIS control: `4004="Háttérben"` sits in the copy-dialog button strip right next to `4005="Sorba állít"`
  (Queue) and `4002="Mégse"`, so the pile ships the exact two-state pair Cmdr mirrors, and the catalog's `Sorba` is
  already the short form of TC's `4005`. Double Commander agrees on the form (`Háttérben futtatás` = "Work in
  background", `Ha az alkalmazás a háttérben fut`), as does Microsoft (`background task` = `háttérben futó feladat`) ·
  high. No macOS tier for this sense: Finder has no such control and `hu/macOS/` holds `háttér` only in the backdrop
  sense (`Háttérkép`, `háttérszín`), so the Tier-1 tiebreak is absent (mining gotcha 2), not missing.
  - **NOT the illative `Háttérbe`**, even though it would look more parallel to `Sorba`: Hungarian puts work INTO a
    queue (`sorba állít`) but runs it IN the background (`háttérben futtat`), the illative has ZERO attestation across
    the whole `hu` pile (only the unrelated adjective `háttérbeli`), and `háttérbe helyez`/`szorít` idiomatically means
    "sideline, deprioritize" — the opposite of the promise that the transfer keeps running.
  - NOT the bare noun `Háttér`: that's the backdrop (`Háttérszín`, `Háttérkép`). The inessive is the case-inflected,
    non-noun short form the sibling `Sorba` establishes for this button.
- **"Keep this running in the background" (`.backgroundAria`) → `Hagyd futni a háttérben`** · REUSED verbatim: this
  exact English sentence is already the first clause of the tooltip on the SAME button (`queueTooltip` =
  `Hagyd futni a háttérben, és kezeld a műveleti sorban (F2)`) and closes `transferProgress.stallUnknown`. One sentence,
  one rendering; the aria is the tooltip's opening clause, exactly as in English · high. Informal `te` imperative per
  Formality, no period (matching `queueAria`).
- **WCAG 2.5.3 containment**: the aria ends in `a háttérben`, so the visible label `Háttérben` is a whole-word substring
  of it (case-insensitively, the same bar English meets with "Background" ⊂ "…in the background"; a capital mid-sentence
  would be ungrammatical in Hungarian). Choosing the label's CASE FORM to be the one the natural aria sentence already
  uses is what makes this free — see `style.md` § Notes and decisions. The sibling pair holds the same way: `Sorba` ⊂
  `Áthelyezés a műveleti sorba`.
- No `sameAsSourceJustification` needed: both values differ from English.

Settled while translating the quit gate (7 keys in `main.json`, 2026-08-10). The backend refuses to quit silently while
a copy, move, delete, trash, or archive edit is running, so a modal asks whether to go ahead, lists the running
operations, and counts 15 seconds down to an automatic quit. The head noun `művelet` and the window it points at are
settled in the operation-queue rename block above; these keys only reuse them.

- **quit → `kilépés`; the button "Quit now" → `Kilépés most`** · macOS Tier 1 throughout (`Kilépés`,
  `Kilépés a Finderből`, `Kilépés mindenképp`, `Kilépés és az ablakok megtartása`), Microsoft terminology (`quit` =
  `kilépés`, `Exit` = `Kilépés`), Double Commander (`E&xit` = `Kilépé&s`, `E&xit program` = `Kilépés a &programból`),
  Total Commander (`Alt+F4 Kilépés`) · high. The `<Noun> most` shape carrying "now" is macOS Tier 1 as well
  (`Biztonsági mentés most`, `Letöltés most`), and it keeps English''s load-bearing "now": the app quits either way,
  this button skips the wait. NOT macOS''s `Kilépés mindenképp` ("Quit anyway"): that answers a refusal, while Cmdr''s
  dialog is a countdown the button short-circuits.
- **"an operation is running" (the state, in running prose) → `folyamatban van`; the heading "Still running" →
  `Még folyamatban`** · macOS Finder Tier 1 ships this exact surface, a quit blocked by unfinished file operations:
  `A Finder nem képes kilépni, mert néhány művelet még folyamatban van.` (plus
  `… mert egy művelet még folyamatban van egy iOS-eszközön.` and
  `… mert egy másik művelet van folyamatban, mint például egy elem mozgatása vagy másolása`), and the verbless heading
  form is macOS-attested too (`Első biztonsági mentés folyamatban`); Microsoft terminology agrees (`in progress` =
  `folyamatban`) · high.
  - **NOTE the register split with `queue.row.status` running = `Fut`**, which stays as it is: `Fut` is a one-word
    status cell in a table column, `folyamatban van` is the running-prose form, and macOS uses exactly this pair of
    registers itself. Same shape as the settled `host` split (`gép`/`Gépnév` in the browser column vs `gazdagép` in
    error prose). Don''t "unify" them.
  - The heading is deliberately verbless: `Még fut` would be singular while the list holds 1..N rows, `Még futnak`
    breaks at one row, and `Futó műveletek` would repeat the noun the title just used (English avoids that with the
    terse "Still running").
- **The title is a second-person question: `Kilépsz, amíg egy művelet folyamatban van?`** · every question-shaped title
  in the shipped `hu` catalog uses informal `te` per `style.md` § Formality (`Törlöd az AI-modellt?`,
  `Elküldöd az összeomlási jelentést?`, `Megváltoztatod a fájlkiterjesztést?`, `Mindenképp bezárod?`) · high. Counted
  noun singular in BOTH plural branches (`{countText} művelet folyamatban van`, never `műveletek`), and the verb stays
  singular with it, per the no-pluralize-after-a-numeral rule.
- **"Keep working" (the button that calls the quit off) → `Munka folytatása`** · no source names this control (mining
  gotcha 3: the concept is absent from macOS, Microsoft, and all five file managers, none of which offers a
  stay-in-the-app button on a quit countdown), so it is composed from macOS''s own Tier-1 `<Noun> folytatása` shape
  (`Biztonsági mentés folytatása`, `Másolás folytatása`) plus the nominal-label rule · tentative. FLAGGED.
  - Deliberately NOT `Mégsem`, the catalog''s settled dialog Cancel: next to a list of running operations it would read
    as "cancel the operations", the exact opposite of what the button does.
  - Deliberately NOT `Később` (the settled dismiss-for-now word) or any "remind me" wording: the countdown is deleted,
    not deferred.
  - Residual risk a reviewer should judge: `Folytatás` alone is the queue''s Resume, so `Munka folytatása` could be read
    for a moment as resuming an operation. The object `munka` (the user''s work, not an operation) is what separates
    them, and English carries the same overlap ("Keep working" vs "Resume").
- **"in {n} seconds" → `{secondsText} másodperc múlva`** · the postposition `múlva` is the only correct Hungarian for
  this and needs no suffix on the placeholder (per `style.md` § Notes and decisions); zero pile attestation, since no
  corpus counts a quit down · high on the grammar, and `másodperc` itself is macOS Tier 1 (`Kb. ^0 másodperc`). Singular
  `másodperc` in both plural branches.
- **restart / logout (the operating system''s, not Cmdr''s) → `újraindítás` / `kijelentkezés`** · macOS Tier 1
  (`Újraindítás`, `Kijelentkezés`, `Kijelentkezés…`) and Microsoft terminology (`restart` = `újraindít`,
  `log off`/`log out`/`sign out` = `kijelentkezik`, `Sign Out` = `Kijelentkezés`) · high. Lowercase mid-sentence, as
  Hungarian sentence case requires.
- **The countdown''s "so … never waits on Cmdr" → `így egy újraindítás vagy kijelentkezés soha nem vár a Cmdrre.`** ·
  indicative `így …` rather than a subjunctive `hogy … ne …`, matching English''s plain "so … never waits" and reading
  lighter · high. The brand suffix `Cmdrre` follows `style.md`''s hyphen-free, front-vowel pattern (`Cmdrben`,
  `Cmdrből`, `Cmdrnek` in the shipped catalog), with the `r` doubled by the sublative `-re`.
- **"what it leaves half-written" → `minden félig megírt fájlt`** · `félig megírt fájl` is lifted verbatim from the
  shipped catalog, where the identical English phrase already renders this way
  (`settings.advanced.showStagingTempFiles.description` = `… nem hagyhat félig megírt fájlt valódi néven`) · high.
  `minden` + singular is the Hungarian generic, which keeps the settled phrase while staying number-neutral (see below).
  **"clears away" → `eltávolít`**, the catalog''s and Microsoft''s `remove` = `eltávolítás`, chosen over `törli`: the
  sibling `transferProgress.rollbackTooltip` uses `törlése` for the same cleanup, but that is a destructive-action
  button label, while this sentence is reassurance and must not flash "Cmdr deletes a file" at the reader · high.
- **"anything still being written" → `Ami éppen íródik`** · **the body must stay number-neutral**: one operation writes
  several files at once and several operations can run at once, so a definite singular (`Az éppen írás alatt álló elem`)
  states something false · high. `íródik` over the participial `írás alatt álló` only to keep `áll` out of a clause that
  already ends in `ott áll meg`. "stops where it is" → `ott áll meg, ahol tart`.
- **"Whatever''s finished stays done." → `Ami elkészült, az kész marad.`** · reuses the settled `Done` = `Kész`
  (`queue.row.status` done arm) · high.
- **`countdownAria` → `Hátralévő idő a Cmdr automatikus kilépéséig`** · nominal, like every other aria label in the
  catalog (`Ennek a műveletnek a szüneteltetése`); `hátralévő idő` is the Microsoft-attested shape (`remaining duration`
  = `hátralévő időtartam`, `remaining work` = `hátralévő munka`), and `automatikus` carries "on its own" · high. **Not
  bound by WCAG 2.5.3**: it names a countdown REGION, not a control with a visible label, so there is no label to
  contain (the visible text is the countdown sentence itself).
- No `sameAsSourceJustification` needed: all seven values differ from English.

### Usage stats: "névtelen" dropped, "egy véletlenszerű azonosító" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, 2026-08-12)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `álnevesített` / `pszeudonim` — that
jargon is exactly what the copy avoids.

- **usage stats → `használati statisztika`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the
  `névtelen` adjective was cut. MS terminology's `használati adatok` is the data sense; the statistics reading fits the
  UI · high
- **a random id → `egy véletlenszerű azonosító`** · MS terminology (random → `véletlenszerű`, identifier → `azonosító`)
  · high. `azonosító` is ordinary Hungarian, not jargon.
- **tied to → `-hoz/-hez/-höz kötődik`** · takes the harmonized case suffix on the noun it attaches to
  (`az azonosítóhoz kötődik`, `a nevedhez kötődik`); no placeholder is involved, so the suffix is safe here · high
- **`Mac` + case suffix takes a hyphen: `Mac-eden`** · the written final `c` doesn't spell the pronounced /k/, so AkH's
  hyphen rule applies. Already what `onboarding.stepBeta.emailNote` ships; `settings.updates.emailPrivacyNote` now
  matches it (its old `a Macedre tárolva` was both unhyphenated and in the wrong case for `tárol`) · high
- No `sameAsSourceJustification` needed: every value differs from English.

### Visszagörgetés-megerősítő + a válaszra váró sor állapota (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`, 2026-08-13)

A futó másolás/áthelyezés `Visszagörgetés` gombja most megerősítést kér, a műveleti sor pedig külön állapotot mutat, ha
egy sor azért állt meg, mert a főablakban kérdés vár a felhasználóra.

- **"Needs your answer" (sorállapot) → `Válaszolnod kell`** · a pile közvetlen találata a fogalomra a Double Commander
  műveletnézete (`Waiting for user response` = `Várakozás felhasználói válaszra`), de az en `@key` kiköti, hogy ez az
  állapot NE legyen összetéveszthető a `queued` ággal (`Várakozik`), és a DC alakja pont a `vár-` tővel kezdődik · high
  a `válasz` tőre, `tentative` a formára. A `Válaszolnod kell` a `te`-regisztert használja (style.md § Formality), ahogy
  az angol is közvetlenül szólítja meg a felhasználót ("your"), és két szó a szűk oszlopban.
  - ❌ NEM `Válaszra vár`: idiomatikus, de a `vár` miatt egy pillantásra a `Várakozik` ággal mosódik össze.
  - ❌ NEM `Választ kér`: a `választ` egyben a `választ` ige alakja is, tehát homográf-félreolvasás kockázata.
- **`awaitingAnswerTooltip` → `Válaszolj a kérdésre a főablakban, és ez a művelet folytatódik.`** · a `válaszol` ige a
  testvér `operationConflict.pausedNote`-ból jön (`amíg nem válaszolsz`), a `főablak` a szótár szava
  (`queue.row.foregroundAria` = `Megjelenítés a főablakban`); a "prompt" itt `kérdés`, mert a megerősítő szövegek is
  ezzel a szóval beszélnek róla · high.
- **`rollbackConfirm.title` → `Visszagörgeted ezt a műveletet?`** · a katalógus minden kérdés-címe `te`-alakú, definit
  ragozással (`Törlöd az AI-modellt?`, `Megváltoztatod a fájlkiterjesztést?`) · high. A `visszagörget` a szótár szava.
- **`rollbackConfirm.body` → `Ez törli az összes fájlt, amit a művelet eddig kiírt. Amit felülírt, az nem jön vissza.`**
  · a "written" a katalógus `ki van írva` alakja (`transferProgress.stallInFlight`), a "so far" mindenütt `eddig`
  (`queryUi.results.live.matchesSoFar`, `search.imageResults.paused`), a `felülír` a szótár `overwrite` szava, macOS
  Tier 1 (`Felülírás a célhelyen`) · high. A második mondat szabad vonatkozói szerkezet (`Amit felülírt, az …`), hogy
  szám-semleges maradjon: az angol "any file" sem egy konkrét fájlról beszél.
- **`rollbackConfirm.keep` ("Keep them", a biztonságos válasz) → `Fájlok megtartása`** · macOS Tier 1 a
  `<Főnév> megtartása` alakra (AppKit `Keep` = `Megtartás`, `Mindkettő megtartása`, `Az összes megtartása`,
  `Letöltött megtartása`) · high. A puszta `Megtartás` azért nem elég: a törzsszöveg utolsó mondata a FELÜLÍRT fájlokat
  nevezi meg, így a tárgy kimondása nélkül egy pillanatra rossz tárgyra vonatkozhatna.
- **`rollbackConfirm.rollBack` → `Visszagörgetés`** · szó szerint az a gomb, amelyik a párbeszédet nyitotta
  (`transferProgress.conflictRollback`); az en `@key` kifejezetten kéri az egyezést · high.
- **`transferProgress.rollbackTooltip` (új angol: "Stop, and delete every file written so far") →
  `Leállítás, és minden eddig kiírt fájl törlése`** · a `Leállítás` macOS Tier 1 az abbahagyásra (`Másolás leállítása`,
  `Kettőzés leállítása`, `Leállítás…`), és a katalógus is ezt használja (`queryUi` `Keresés leállítása`) · high.
  Szándékosan NEM `Megszakítás`: az a futó művelet Cancel-szava, az en `@key` viszont pont azt köti ki, hogy a tooltip
  ne olvasódjon sima Cancelként.
- **`transferProgress.foregroundBusyToast` (új angol: "Something else is open here. Close it, then bring this one up.")
  → `Itt valami más van nyitva. Zárd be, aztán hozd elő ezt.`** · az új angol szándékosan nem állítja, hogy a blokkoló
  egy másik MŰVELET (lehet Új mappa vagy törlés-megerősítés is), így a korábbi `Egy másik művelet …` kezdet hamis lett;
  az `itt` a most előtérbe hozott főablak · high. Informal `te` a két felszólító alakban.
- Nem kell `sameAsSourceJustification`: mind a nyolc érték eltér az angoltól.
