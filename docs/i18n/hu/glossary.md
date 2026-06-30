# hu glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/hu/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: [i18n-translation.md](../../guides/i18n-translation.md).

## Terms

Core UI terms (pane, tab, volume, drive, folder, file, move, copy, rename, delete, trash, cancel = `Mégsem`, eject,
disconnect, share, search, sort, settings, index, overwrite, server = `szerver`) are sourced and fixed in
[`style.md`](style.md) § Terminology and glossary; use those verbatim. Below are the terms settled while translating
`fileExplorer.json` (first pass, 2026-06-21).

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
  generic `várólista`. Window title `queue.windowTitle` = `Átviteli sor`; the command `commands.queueShow.label` =
  `Átviteli sor megjelenítése`; empty state "Nothing in the queue" = `A sor üres`. The progress-dialog "Queue" button
  (sends the transfer to the background and opens the queue window) = `Sorba` (short label, "into the queue"; mirrors DC
  `A&dd To Queue` = `Várakozási &sorba helyez`); its aria "Send to the transfer queue" = `Áthelyezés az átviteli sorba`.
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
- preset (value in a settings-picker dropdown) → előbeállítás; "back to presets" → "Vissza az előbeállításokhoz" (allative -hoz, vowel-harmonized) · Double Commander hu ("előbeállítás": "módosított előbeállítással", "a … előbeállítást") · high

Settled while translating the four copy/delete-dialog polish keys (`fileOperations.json`, 2026-06-30):

- "Action:" (field label before the Copy/Move or Trash/Delete segmented control, `shared.actionLabel`) → `Művelet:` · ms terminology ("action" = `művelet`, Noun), macOS Finder ("This action cannot be performed." = "Ez a művelet nem hajtható végre.") · high. Matches the settled `File operations → Fájlműveletek` (művelet = operation/action). Sentence case, trailing colon kept.
- "Route:" (field label before a "source → destination" line in the copy/move dialog, `transferDialog.routeLabel`) → `Útvonal:` · ms terminology ("route" = `útvonal`, network-address and path/road senses both) · high. The from→to of the transfer; `útvonal` is the natural Hungarian word for "route". Note it shares the word with `path → útvonal`, but they're separate labels on separate lines (this is the whole source→dest route; `destPathAria` = `Célútvonal` is the destination field), so no in-screen clash. Trailing colon kept.
- "Scanning…" (spinner tooltip + SR label WHILE counting selected items, `shared.scanningTooltip`) → `Átvizsgálás…` · in-file consistency with `transferProgress.stageScanning` = `Átvizsgálás`, glossary `scan (index) → átvizsgálás`, ms ("scan" = examine files/data = `vizsgál`) · high. Ellipsis `…` kept (single char, per the typography reconciliation).
- "Scan complete" (checkmark tooltip + SR label once counting FINISHED, `shared.scanCompleteTooltip`) → `Átvizsgálás kész` · reuses the `Átvizsgálás` scan term + settled `Done → Kész` · high. "Scanning finished/done" reads naturally and stays terse for a tooltip.

Settled while translating the destination-will-be-created warning keys (`fileOperations.json`, 2026-06-30):

- "This folder doesn't exist yet. Cmdr will create it during the copy/move." (yellow inline warning under the destination box when the typed dest folder doesn't exist, `transferDialog.targetWillBeCreatedCopy`/`…Move`) → `Ez a mappa még nem létezik. A Cmdr létrehozza a másolás során.` / `… az áthelyezés során.` · `nem létezik` for "doesn't exist" is Total Commander / Double Commander's exact phrasing for a missing target dir ("A(z) „%s” könyvtár nem létezik. Létrehozza?"); `még nem létezik` adds the "yet"; "during the copy/move" = `a másolás során` / `az áthelyezés során` (Double Commander "másoláskor"/"… során"); reuses settled `folder → mappa`, `copy → másolás`, `move → áthelyezés`, `create → létrehoz` (`létrehozza` = definite conj., the "it" object folded in) · high. Two literal sentences per the en `@key` note (operation-specific verb, no ICU select). Brand `A Cmdr` bare as subject (consonant onset → article `a`).
