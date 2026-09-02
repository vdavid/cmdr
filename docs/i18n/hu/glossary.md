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
  **SUPERSEDED for the plain "problem" sense as of 2026-08-23: use `probléma`.** `gond` has ZERO attestation anywhere in
  the `hu` reference pile; the evidence and the full argument are in § Ha a Cmdr nem állt le.
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
GB, the `{placeholder}`-only path strings (`{systemSettings} > {appearance}`, the permission path).

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
- zoom (the level, and the View submenu holding it) → `Nagyítás` · ms · high. "Zoom to 100%" = `Nagyítás 100%-ra`; zoom
  level = `nagyítási szint`. The zoom in/out STEPS are `Felnagyítás` / `Lekicsinyítés` (see § Natív menük), which keeps
  `Nagyítás` free as the submenu's own title.
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
- **Example email placeholder → `te@example.com`** (see § A példa e-mail-cím lentebb; a helyi rész magyar, a domain
  marad `example.com`).

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

Egy kulcs: a ⌘⌥C utáni információs toast szövege. Maga az útvonal alatta, külön, fix szélességű sorban jelenik meg,
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

### Az átnevezés-láncban nevüket megtartó fájlok számláló buboréka (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, 2026-08-18)

A soron belüli átnevezőben a fel/le nyíllal végigfutó átnevezésekből több is elmaradt. Ugyanaz az EGY buborék, mint a
`chainKeptOriginalName`, csak újraírva: megnevezi a legutóbbi fájlt, a korábbiakat pedig megszámolja.

- **A testvérkulcs első tagmondata szó szerint marad**: `A(z) „{name}” megtartotta a nevét`. A buborék a helyén íródik
  újra a felhasználó szeme előtt, ezért csak a farka nőhet, ahogy az angolban is. Az idézőjel `„…”` (style.md), a
  `megtartotta a nevét` a testvérkulcs igéje.
- **"and so did {N} other files" → `és még {othersText} másik fájl is.`** · a `„^1” és ^0 másik elem` alak macOS Tier 1
  (Finder AirDrop: `„^1” és ^0 másik elem fogadása.` / `… küldése.`, valamint az egyesítés-párbeszéd
  `a(z) „^1” és ^0 további elem`), tehát az `és {szám} másik {főnév}` szerkezet és a szám utáni EGYES SZÁMÚ főnév is
  attesztált · high. A záró `is` (az azonos állítmány elhagyása, magyar gapping: „Péter megtartotta a nevét, és Anna
  is.”) pont az angol "and so did" megfelelője, ezért nem kell megismételni az igét · a NYELVTANRA high, a FORMÁRA
  tentative: a pile rövid címkékből és hibaüzenetekből áll, egyetlen ` is.`-re végződő mondat sincs benne, tehát erre a
  záró alakra nincs korpuszfedezet.
  - ❌ NEM összevont alany (`A(z) „{name}” és még {N} másik fájl megtartotta a nevét.`): rövidebb ugyan, de az en `@key`
    kiköti, hogy a `{reason}` CSAK a megnevezett fájlra vonatkozik, az egy alannyá olvasztás pedig az egész csoportra
    vinné át az indoklást.
  - ❌ NEM `és ugyanez történt még {N} másik fájllal`: nyelvtanilag rendben van, de a pile-ban a `történt` szinte
    kizárólag a `Hiba történt …` / `… hiba történt` fordulatban él (macOS AppKit 8 találatból 6, Nautilus, Thunar,
    Double Commander), és a buborék hangja szándékosan kerüli a hiba-regisztert (style.md § Voice and tone).
- **Az `one` ág kiírja a számnevet: `egy másik fájl`**, ahogy az angol is ("one other file") és a
  `settings.mediaIndex.reclaim.line` mintája; az `other` ág a `{othersText}` formázott értéket használja. A főnév
  MINDKÉT ágban egyes számú (`másik fájl`, soha `másik fájlok`) a szám utáni nem-többesítés szabálya szerint · high.
- Nem kell `sameAsSourceJustification`: az érték eltér az angoltól.

### A meg nem erősített átnevezés buboréka és a fel nem használható név (`fileExplorer.rename.unconfirmed`/`unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`, 2026-08-18)

Lassú köteten (hálózati megosztás, telefon) az átnevezésre nem érkezik visszajelzés időben. A buborék NEM állíthatja,
hogy a fájl megtartotta a nevét: azt mondja, hogy a Cmdr nem tudja. Ez a `chainKeptOriginalName*` pár testvére, de a
JELENTÉSE ellentétes, ezért a nyitó tagmondat szándékosan más.

- **"Couldn't confirm X" → `Nem sikerült megerősíteni, hogy …`** · a katalógusban EZ a család bevett nyitánya ugyanerre
  a meg-nem-erősített-művelet helyzetre: `fileExplorer.pane.trashUnconfirmedToast`
  (`Nem sikerült megerősíteni, hogy a fájl a Kukába került.`) és `fileOperations.mkdir.timeoutMessage`
  (`Nem sikerült megerősíteni, hogy a mappa létrejött.`) · high.
  - ❌ NEM tárgyas `Nem sikerült megerősíteni a(z) „X” átnevezését`: a pile-ban a `megerősítés` tárgyas alakja kizárólag
    a jóváhagyás-értelmet viszi (Double Commander `Felülírások megerősítése`, `megerősítés kérése nélkül`; Nautilus
    `Jelszó megerősítése`; macOS AppKit `Confirm` = `Megerősítés`), a `hogy`-os mellékmondat viszont egyértelműen az
    ellenőrzés-értelem.
- **"the rename of X" → `a(z) „X” átneveződött` (mediopasszív)** · a testvér buborék pontosan ezt az alakot használja a
  meg nem erősített műveletre (`trashUnconfirmedToast`: `a fájl így is áthelyeződhetett`), és az `-ódik/-ődik`
  mediopasszív a pile-ban is a gépi ágens nélküli állítás alakja (Nautilus, Thunar, Dolphin: `törlődnek`, `másolódnak`,
  `mentődnek`) · high a szerkezetre, `tentative` a szótőre: konkrétan `átneveződ*` alak sem a pile-ban, sem a
  katalógusban nincs.
  - ❌ NEM `átnevezték` (a katalógus 3. sz. többes határozatlan alakja, pl. `errors.write.sourceNotFound.suggestion`):
    az KÜLSŐ ágenst jelöl (valaki a Cmdren kívül nevezte át), itt viszont maga a Cmdr nevezett át.
- **"The volume may be slow" → `Lehet, hogy a kötet lassú`** · szó szerint a két testvér buborék második mondata
  (`trashUnconfirmedToast`, `mkdir.timeoutMessage`); `kötet` a szótár szava (macOS Tier 1), a `lassú` melléknévre
  Nautilus (`A keresés lassú lehet, …`) és Double Commander (`(lassú)`, `(lassabb)`) a fedezet · high.
- **"the rename may still have gone through" → `így az átnevezés attól még sikerülhetett`** (többesben
  `az átnevezések attól még sikerülhettek`) · az `attól még` + `-hat/-het` potenciális alak szó szerint a
  `mkdir.timeoutMessage` mintája (`így a mappa attól még létrejöhetett`) · high a szerkezetre, `tentative` a
  `sikerülhetett` alakra: a `sikerül` ige a pile-ban 88-szor, a katalógusban 100-szor szerepel, de szinte kizárólag a
  tagadó `nem sikerült` fordulatban, erre az állító potenciális alakra nincs korpuszfedezet.
  - **Az alanyt KI KELL mondani** (`az átnevezés`), pro-drop itt hibás: a második mondat élén `a kötet` az utolsó
    alanyeset, így a `így attól még átneveződhetett` egy pillanatra „a kötet neveződhetett át” olvasatot ad. A két
    testvér pontosan ezért ismétli meg a maga alanyát (`a mappa`, `a fájl`).
  - Az alany `az átnevezés`, nem `a fájl`: az `unconfirmed` kulcs alatt MAPPA is állhat, tehát a testvérek főneve itt
    hamis állítás lenne. Az angol is ezt a főnevet nevezi meg ("the rename(s)").
  - ❌ NEM `megtörténhetett`: a pile-ban a `történt` szinte kizárólag a `Hiba történt …` fordulatban él (lásd a fenti
    `chainKeptOriginalNameAndOthers` blokkot), a buborék hangja pedig kerüli a hiba-regisztert.
- **Az `AndOthers` számnév-ágai szó szerint a `chainKeptOriginalNameAndOthers`-éi** (`egy másik fájl` /
  `{othersText} másik fájl`), hogy a két buborékpár egy hangon szóljon. Az összetett alany után az első tagmondat egyes
  számú állítmányt kap (`átneveződött`, a magyar alapeset számnévi tag után), a második viszont többeset
  (`átneveződhettek`), hogy a lehetőség az EGÉSZ csoportra vonatkozzon, ne csak az utolsó tagra · high.
- **"That filename can't be used" → `A fájlnév nem használható` / `A mappa neve nem használható`** · a `nem használható`
  névre alkalmazva macOS Tier 1 (Finder `A(z) „^0” név nem használható.`, `… mert túl hosszú.`,
  `… mert a rendszer számára van fenntartva.`; AppKit Document `A(z) „%@” név nem használható.`) · high. A főnév a
  testvérkulcsokéval azonos (`validation.empty`, `.disallowedChars`, `.nameTooLong`: `A fájlnév` / `A mappa neve`). Záró
  pont nincs: az érték hosszabb mondatba épül be (`{reason}. A(z) „{name}” megtartotta a nevét.`).
  - Az angol `That` mutató névmása elmarad: a `mappanév` összetétel a pile EGYIK forrásában sem szerepel (a `fájlnév`
    60+ találattal igen), az `Ez a mappa neve …` pedig félreolvasható „ennek a mappának a neve” értelemben. A magyar
    határozott névelő amúgy is a beírt névre mutat.
- Nem kell `sameAsSourceJustification`: mindhárom érték eltér az angoltól.

## Javasolt műveletek: az Ask Cmdr javaslatainak ablaka (`suggestedOps.*`, `commands.suggestedOpsShow.*`, 2026-08-19)

- ops (az ügynök által javasolt fájlműveletek) → `műveletek`; a cím `Javasolt műveletek` · a katalógus `fájlművelet`
  szóhasználatához igazítva · high
- approve → `Jóváhagyás` · ms; a macOS `Elfogadás` az AirDrop-elfogadás párja, itt viszont engedélyezésről van szó ·
  high
- reject → `Elutasítás` · macOS Finder AirDrop-panel (Tier 1) · high
- "This can't be undone" → `Ezt nem lehet visszavonni` · macOS Finder ("Ezt a műveletet nem vonhatja vissza"),
  tegező-semleges alakra hozva, mert a Cmdr tegez · high
- pattern → `minta` · már a katalógusban (`queryUi.json`) · high
- undo → `visszavonás` · már a katalógusban (`askCmdr.renameUndo`) · high

## Megkettőzés: a parancs, amely ugyanabba a mappába másol (`commands.fileDuplicate.*`, 2026-08-19)

- **duplicate (a kijelölést a saját mappájába másoló parancs) → `Megkettőzés`** · macOS Finder `hu`, „Fájl >
  Megkettőzés” (`N154`), valamint „Elemek megkettőzése” és „Megkettőzi az elemeket a jelenlegi helyükön” (ellenőrizve
  macOS 26.6.1 alatt, `Finder.app/Contents/Resources/hu.lproj`, 2026-08-19) · high. A névszói alak illeszkedik a
  `Másolás` / `Áthelyezés` / `Átnevezés` sorhoz, és egyikkel sem keveredik.
- **„Make a copy of the selected files in the same folder” →
  `Másolat készítése a kijelölt fájlokról ugyanabban a mappában`** · a szomszédos leírások névszói alakja („Kijelölt
  fájlok másolása…”); a `másolatot készít valamiről` vonzat a természetes magyar szerkezet, az „ugyanabban a mappában”
  pedig arra a mappára utal, amelyben a fájlok már benne vannak · high.

## Natív menük: menüsor, helyi menük, ablakcímek (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, 2026-08-19)

A csoport forrásai: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/hu.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) a Tier 1, és szinte mindent eldönt; az angol oldal az `en_GB.lproj`-ban van, mert a
`Base.lproj` csak lefordított nibeket tartalmaz. A Safari 26 (`MainMenu.strings`) adja a lapokra vonatkozó
szóhasználatot, a Microsoft-terminológia azt, aminek az Apple-nél nincs neve. RAW család: **egyszeres aposztróf**, a
`''` a menüben kettőnek látszana.

- **Menüsor: `Fájl`, `Szerkesztés`, `Nézet`, `Ugrás`, `Ablak`, `Súgó`, `Szolgáltatások`** · macOS Finder és Safari `hu`
  · high.
- **pane → `panel`, immár `high` (eddig `tentative`)** · Double Commander `hu` („Bal panel”, „Jobb panel”) és Total
  Commander `hu` („a célpanelben”, `WCMD.INC` 911) · high. Az ortodox kétpaneles pár a Cmdr közvetlen rokona, tehát ez a
  megfelelő családból származó bizonyíték; a `style.md` nyitott kérdései közül ez lezárult.
- **Select menü (fájlkijelölés) → `Kijelölés`** · Nautilus `hu` („Kijelölés”), és illeszkedik az `Összes kijelölése`
  sorhoz · high.
- **Deselect all → `Kijelölés törlése`** · macOS Finder `hu` (`300488.title`) · high. A Finder szóhasználata, nem a
  katalógus korábbi `Kijelölés megszüntetése` alakja; a `Fájlok kijelölésének törlése…` ennek a párja.
- **Go > Home → `Saját`** · macOS Finder `hu` (`253.title`) · high. Rövid, és pontosan ezt látja a felhasználó a
  Finderben.
- **Window > Zoom → `Méretezés`, Minimize → `Minimalizálás`** · macOS Finder `hu` · high.
- **zoom in / out → `Felnagyítás` / `Lekicsinyítés`** · Safari `hu` (Nézet menü) · high. Így a `Nagyítás` szabadon marad
  a zoom-almenü címének, és nem ütközik a saját elemével.
- **Quick Look → `Gyorsnézet`** · macOS Finder (`TL14`) · high. Az Apple lefordítja ezt a funkciónevet, ezért nincs a
  ne-fordítsd listán.
- **ascending / descending → `Növekvő` / `Csökkenő`** · Thunar + Dolphin `hu` · high.
- **changelog → `Módosítási napló`** · Microsoft-terminológia · high. Elkülönül a Súgó > `Újdonságok` elemtől: az egyik
  a dokumentumot nevezi meg, a másik a hírt.
- **word wrap → `Sortörés`** · Microsoft-terminológia · high.
- **pin / unpin tab → `Lap rögzítése` / `Lap rögzítésének feloldása`** · Safari `hu` („Lap rögzítése”) · high.
- **„Edit in editor” → `Megnyitás szerkesztésre`** · leíró · tentative. A szó szerinti „Szerkesztés szerkesztőben”
  ismétlődik, mert magyarul az `edit` és az `editor` ugyanabból a tőből jön; a megnyitás-szerkesztésre szerkezet
  idiomatikus, és megkülönböztethető a fölötte álló `Megtekintés`-től.
- **„Don't index images in this folder” / „Index images here again” → `Képek indexelésének tiltása itt` /
  `Képek indexelésének engedélyezése itt`** · névszói címkealak, a magyar UI-konvenció szerint · tentative. A tiltó
  felszólító mód (`Ne indexeld…`) tegező közvetlen megszólítás lenne, amit a címkéknél a `style.md` kerül.
- **Finder-címkeszínek → `Piros, Narancs, Sárga, Zöld, Kék, Bíbor, Szürke`** · macOS Finder (`TG_COLOR_*`) · high.
- **busy (használatban lévő kötet) → `(foglalt)`** · Microsoft-terminológia (`foglalt` = vonal foglalt) · high.
- **Eject → `Kiadás`, Disconnect → `Leválasztás`, Remove (listából) → `Eltávolítás`** · macOS Finder · high.
- **A márkanév toldalékolása: `Kilépés a cmdrből`** · a `style.md` kötőjel nélküli, kiejtés szerinti szabálya
  („commander” → elöl képzett magánhangzók → `-ből`) · high. Az `en` érték szándékosan kisbetűs `cmdr`, ezért a magyar
  is az marad.
- **Szándékosan azonos az angollal** (`sameAsSourceJustification`): `menu.zoom.percent*` és `menu.view.askCmdr`.

### Rendszerkapcsolatra visszaeső SMB-buborék (`fileExplorer.network.osMountFallback.*`, 2026-08-21)

Három kulcs: a buborék szövege, a benne lévő gomb és a bezáró X elemleírása. Akkor jelenik meg, amikor a Cmdr saját,
gyorsabb kapcsolata nem jött létre, és a megosztás a macOS kapcsolatán fut. A hangnem megnyugtató, nem riasztó: a
megosztás működik, csak lassabb.

- **native (a macOS saját SMB-kapcsolata) → `natív`** · Microsoft-terminológia (Tier 2, `native` = `natív`, melléknév,
  HUN; `natív mód`, `natív formátum`, `natív fájl`) · high. A macOS `hu` sehol nem használja a szót (az Apple kerüli),
  tehát nincs Tier-1 ellenbizonyíték. A `beépített` NEM ennek a szava: az a `built-in` fordítása, és így is szállítjuk
  (`settings.mediaIndex.privacyNote` „Apple beépített Vision keretrendszerével”).
- **Ugyanezt a kapcsolatot a katalógus máshol `rendszerkapcsolat`-nak hívja** (`smbNativeNote` ×2, a négy
  `pane.directConnection*Toast`), mert ott az angol is „system connection”. Itt az angol kifejezetten megnevezi a
  macOS-t és az SMB-t, ezért a hosszabb, névadó alak marad. Ugyanaz a dolog, két angol megnevezés.
- **„Couldn''t directly connect to X” → `Nem sikerült közvetlenül csatlakozni ehhez: X`** · a katalógus saját,
  szállított szerkezete (`network.share.connectFailedTitle` = „Nem sikerült csatlakozni ehhez: {hostName}”,
  `navigation.driveIndex.refusedUpgradeFailed` = „A Cmdr nem tudott közvetlenül csatlakozni ehhez: {name}…”) · high. A
  kettőspontos hely azért kell, mert a `{share}` értéke ismeretlen, tehát nem toldalékolható (`style.md` §
  Agglutination). A `<shareName>` címke a kettőspont utáni névre kerül.
- **„Try connecting directly” (gomb) → `Próbálkozás közvetlen kapcsolattal`** · névszói címkealak (`style.md` §
  Formality), és a fejszava a szállított `navigation.connectDirectly` = „Közvetlen kapcsolat a gyorsabb hozzáférésért”
  szókincse · high a szókincsre, `tentative` a gombalakra. A puszta `Közvetlen kapcsolat` rövidebb lenne, de elveszne
  belőle az angol „Try” óvatossága (a közvetlen kapcsolat egyszer már nem jött létre). Hossz: 34 karakter az angol 24
  helyett, szűk buborékban ez az egyetlen túlcsordulási kockázat a három kulcs közül.
- **Dismiss → `Elvetés`** · VÁLTOZATLANUL átvéve a `lowDiskSpace.toast.closeTooltip` értékéből (azonos `sourceHash`,
  `48845bf`), és ez a katalógus hét helyén szállított `Dismiss` · high.
- **„4x slower … (sometimes 100x)” → `négyszer … (néha százszor is)`**: a szorzószám magyarul kiírva, számjegy nélkül.
  Indoklás és forrás: `style.md` § Notes and decisions, szorzószámok.
- **„for most connections” → `a legtöbb esetben`** (nem `a legtöbb kapcsolatnál`): a mondatban már három `kapcsolat`
  szerepel (`kapcsolatán`, `kapcsolatánál`, plus a gomb), a negyedik magyarul zsúfolt. A jelentés ugyanaz.
- **`sameAsSourceJustification` egyik kulcsnál sem kell**: mindhárom érték eltér az angoltól.
- ⚠️ Már meglévő ellentmondás, NEM ebben a passzban javítva: a `fileOperations.transferDialog.smbNativeNote` idézőjelben
  a „Közvetlen csatlakozás” menüpontra küldi a felhasználót, de a kötetválasztóban valójában
  `navigation.connectDirectly` = „Közvetlen kapcsolat a gyorsabb hozzáférésért” áll. A két hely egy kulcs átírásával
  összehangolható.

## Átnevezés/létrehozás elutasításai: a `errors.mutation.*` és `errors.volume.*` egysoros üzenetek (2026-08-23)

31 kulcs: a név mező alatt vagy egy rövid buborékban megjelenő EGY mondat, amikor egy átnevezés, Új mappa vagy Új fájl
nem megy át. RAW család (nincs ICU), tehát egyszeres aposztróf, és a `{path}` szó szerint marad. A `{path}` értéke
ismeretlen futásidejű útvonal, ezért mindenhol a katalógus bevett kettőspontos, toldalék nélküli helyére kerül
(`itt: „{path}”`, `ehhez: „{path}”`), vagy `A(z) „{path}”` alanyi helyre; idézőjel mindig `„…”` (style.md).

- **locked (a macOS zárolás-jelzője) → `zárolva van`; feloldása → `Oldd fel a zárolását`** · macOS Finder Tier 1 (`PE13`
  = „A művelet nem hajtható végre, mert a(z) „^0” elem zárolva van.”, `NE17` ugyanez fájlra, `AXNODE1` = `Zárolva`, a
  jelölőnégyzet `Zárolt`) · high. A `feloldás` a szótár szava (`archivePassword.unlock`).
- **Get Info (a Finder infóablaka) → `Infó megjelenítése`** · macOS Finder Tier 1 (`N165`, `TL22`, és futó szövegben
  `NE18`/`BN43`: „Válassza a Fájl > Infó megjelenítése parancsot…”) · high. Ez a szótárban már szerepel a
  `commands.json` passzból; itt az ABLAK megnevezéseként használjuk: `a Finder Infó megjelenítése ablakában`. ⚠️ A
  katalógusban két régebbi érték (`errors.write.permissionDenied.suggestion.deleteMac`, `errors.listing.*.suggestion`)
  még az angol „Get Info” alakot írja; egy külön passzban érdemes egységesíteni.
- **System Integrity Protection → `Rendszerintegritás-védelem`** · macOS Finder Tier 1 (`ET6` = „Néhány elem nem
  törölhető a Kukában a Rendszerintegritás-védelem miatt.”) · high. Az Apple lefordítja ezt a funkciónevet, ezért nem a
  ne-fordítsd listára tartozik. A mondat a `védelem alatt áll` bevett magyar vonzatot használja
  (`Ez az elem a macOS Rendszerintegritás-védelme alatt áll…`), így a `véd-` tő nem ismétlődik közvetlenül egymás után.
- **„can't be renamed” → `nem nevezhető át`** · macOS Finder Tier 1 (`RN33`, `RN37` = „A(z) „^0” elem nem nevezhető
  át.”, `RN11`) · high.
- **„isn't available any more” (kötet) → `már nem érhető el`** · macOS Finder Tier 1 (`NE7` = „…mert a(z) „^0” lemez nem
  érhető el többé.”) · high. A `már nem` az idiomatikusabb sorrend ugyanarra a tőre.
- **„didn't answer in time” → `nem válaszolt időben`** · macOS AppKit Tier 1 az `időben` határozóra („…nem fejeződött be
  időben”), a `válasz` tő pedig Total Commander (`Adatküldés, várakozás a válaszra…`) · high. Szándékosan NEM
  `nem reagál`: az a hibás-működés regisztere (lásd a megtorpant átvitel blokkját).
- **„no room left” → `nincs több szabad hely`** · a szállított `errors.listing.storageFull.explanation` („Nincs elég
  szabad hely ezen a köteten…”) és a macOS `PE18` (`a lemez megtelt`) ugyanezt a fogalmat nevezi meg · high. A
  katalógusbeli alakot folytatjuk, hogy a két üzenet egy hangon szóljon.
- **„That password didn't work.” → `Ez a jelszó nem jó.`** · a jelszót minősíti, nem a felhasználót; macOS Tier 1 a
  `helytelen` alakot hozza (`PE77`), a TC/DC pedig a `hibás`/`ROSSZ JELSZÓ` alakot · high a jelentésre, `tentative` a
  formára. Az angol szándékosan a lágyabb „didn't work”-öt választja a „is incorrect” helyett, ezért a magyar sem a
  hivatalos `helytelen`; a `hibás` pedig a `hiba` tő miatt esik ki (style.md § Voice and tone).
- **„The destination can't hold that name.” → `Ez a név nem használható a célhelyen.`** · macOS Finder Tier 1 a névre
  alkalmazott `nem használható` alakra (`A(z) „^0” név nem használható.`), és pontosan ezt használja a testvér
  `fileOperations.validation.nameNotUsable` is (`A fájlnév nem használható`) · high. A `célhely` a szótár szava. Így a
  javítás iránya (másik név, nem újrapróbálkozás) egyértelmű marad.
- **`errors.mutation.timedOut` NEM kudarcként fogalmaz**:
  `A kötet még nem válaszolt, így a módosítás attól még végbemehet.` · szó szerint a
  `fileOperations.mkdir.timeoutMessage` `attól még …-hat` mintája („így a mappa attól még létrejöhetett”) · high. Az
  `attól még` viszi az angol „may still land” engedékeny jelentését.
- **`errors.volume.deviceSessionReset` NEM kihúzásról szól**:
  `Az eszköz újraindította a kapcsolatot. Várj néhány másodpercet, majd próbáld újra.` · a második mondat szó szerint a
  szállított `errors.listing.deviceReconnecting.suggestion` („Várj néhány másodpercet, majd próbáld újra.”), amely
  ugyanezt az MTP-munkamenet-újraindulást magyarázza · high. Az eszköz csatlakoztatva marad, ezért
  `leválasztás`/`kihúzás` szó nem szerepel benne.
- **`errors.volume.deviceDisconnected` (a kapcsolat magától szakad meg) → `Megszakadt a kapcsolat az eszközzel, …`** · a
  szótár `megszakad a kapcsolat a meghajtóval` döntése (hálózati meghajtó lecsatlakozása); a felhasználó által kezdett
  `leválasztás` itt hamis lenne · high.
- **„archive edit” → `az archívum szerkesztése`** · a szállított `queue.row.label` `archive_edit` ága
  (`Archívum szerkesztése`) · high. Kisbetűs `zip` formátumnév kötőjeles összetételben: `zip-archívumok` (szótár,
  helyesírás).
- **„Move it instead.” → `Helyezd át inkább.`** · a tegező felszólító a katalógus más utasításainak alakja
  (`próbáld újra`, `Szakítsd meg…`), és az igető ugyanaz, mint az `Áthelyezés` parancsé, tehát a felhasználó tudja,
  melyik parancsra utal · high.
- **„Something went wrong, and Cmdr couldn't tell what.” →
  `Valami nem sikerült, és a Cmdr nem tudta megállapítani, hogy mi.`** · a `Something went wrong → Valami nem sikerült`
  a katalógus egyeztetett alakja (2026-06-21 passz), a `megállapít` pedig a szótár „work out” igéje · high.
- **„Cmdr stopped this at your request.” → `A Cmdr a kérésedre leállította ezt a műveletet.`** · semleges, nem
  mentegetőző; a `leállít` a macOS Tier-1 abbahagyás-igéje (`Másolás leállítása`), a `művelet` a szótár szava · high. A
  futó átvitel `Megszakítás` gombjától szándékosan eltér: itt nem gombfeliratról van szó.
- Nem kell `sameAsSourceJustification`: mind a 31 érték eltér az angoltól.

Két további kulcs ugyanebbe a családba (Kukába helyezés elutasításai):

- **Trash (a macOS kukája) → `Kuka`, nagybetűvel, a funkció neveként** · macOS Finder/AppKit Tier 1 (`Trash` = `Kuka`,
  „Moves items to the Trash” = „Elemeket helyez át a Kukába”), és a katalógus már settled alakja (`delete.trashSwitch` =
  `Áthelyezés a Kukába`, `errors.write.*.trash` ág) · high. A Windows-os `Lomtár` itt nem jön szóba (macOS-app).
- **„This volume has no Trash, so the only way is to delete permanently.” →
  `Ezen a köteten nincs Kuka, ezért csak a végleges törlés marad.`** · a `kötet` a szótár szava; az `ezen a köteten`
  helyhatározós alak a szállított `errors.volume.storageFull` mintája („Nincs több szabad hely ezen a köteten.”), a
  `végleges törlés` pedig szó szerint a parancs neve (`commands.fileDeletePermanently.label` = `Végleges törlés`), így a
  felhasználó tudja, melyik parancsot keresse · high. A `csak … marad` viszi az angol „the only way is” jelentését
  anélkül, hogy kudarcnak nevezné a helyzetet.
- **„macOS wouldn't move this to the Trash.” → `A macOS nem engedte ezt a Kukába helyezni.`** · a „wouldn't” elutasítás,
  nem kudarc, ezért NEM `nem sikerült` (a style.md tiltja a `hiba`/`sikertelen` címkét); a `nem engedte …` az AppKit
  jogosultsági mondatainak igéjéhez („nincs jogosultsága a fájlt a kukába helyezni”) áll a legközelebb, de általánosabb
  · high. Szándékosan rövid: a technikai ok külön, a „Technikai részletek” alatt jelenik meg. Az `A macOS` névelője a
  kiejtés szerinti („makOS”), ahogy a katalógus máshol is (`errors.mutation.sipProtected`).
- Ennél a két kulcsnál sem kell `sameAsSourceJustification`: mindkét érték eltér az angoltól.

## Ha a Cmdr nem állt le: az összeomlásjelentő két új nyitómondata (`…keptRunning` / `.unknown`, 2026-08-23)

Az összeomlásjelentő párbeszéd nyitómondata már nem egyetlen fix mondat: a jelentés maga rögzíti, hogy a Cmdr leállt-e.
A `.ended` (`váratlanul bezárult`) változatlan; ez a két új kulcs viszont olyan esetet ír le, amelyben a Cmdr NEM állt
le, tehát egyikük sem állíthatja, hogy leállt.

- **„ran into a problem” → `problémába ütközött`** · a `-ba/-be ütközik` szerkezet macOS Finder, Tier 1:
  `A(z) „^0” hibába ütközött.` (`LocalizableMerged`, `NE105`). A főnév `probléma`, mert az Apple
  összeomlás-párbeszédének egész családja ezt használja (`CrashReporterSupport.framework/hu.lproj`:
  `A számítógépe újraindult egy probléma miatt.`, `Grafikai problémát észlelt a rendszer.`,
  `jelentést küldhet a problémáról`; ellenőrizve macOS 26.5.2 alatt, 2026-08-23), a Microsoft-terminológia is
  `probléma`, és a katalógus hangneme kerüli a puszta `hiba` szót (`error → Probléma`) · high.
  - ❌ NEM `gondba ütközött`: a `gond`/`gondba` szóra a teljes `hu` pile NULLA találatot ad (macOS, Microsoft, Nautilus,
    Thunar, Dolphin, Total Commander, Double Commander). A fenti `problem / glitch → gond · tentative` sor ezzel megdől.
  - ❌ NEM az Apple `egy probléma miatt` szerkezete: az mindig leállást jelentő főigét kíván (`újraindult`,
    `nem nyitható meg`), tehát pont azt állítaná, amit ennek a két kulcsnak tagadnia kell.
  - ❌ NEM `Probléma történt a Cmdrben`: a pile-ban a `történt` szinte kizárólag a `Hiba történt …` fordulatban él, és a
    párbeszéd hangja kerüli a hiba-regisztert.
- **„and kept running” → `és tovább futott`** · a katalógus saját, szállított alakja ugyanerre a fogalomra:
  `transferProgress.stallUnknown` „Still running in the background” = `Tovább fut a háttérben`, mellette
  `Hagyd futni a háttérben`. A pile-ban erre a jelentésre NINCS közvetlen találat (`tovább fut`, `továbbra is fut`,
  `fut tovább` mind nulla); a legközelebbi az AppKit `NSExceptionAlert` („…ha szeretné folytatni a futtatást az
  inkonzisztens állapot ellenére”), ami ugyanez a fogalom, de felhasználói döntésként. Az ige `fut` alakját a Double
  Commander is hozza (`Ha az alkalmazás a háttérben fut`) · high a szókincsre, `tentative` a múlt idejű `futott` alakra:
  arra sem a pile-ban, sem a katalógusban nincs fedezet.
- **„in the background” → `a háttérben`** · a szótár szava (`background → háttér`); a futás értelmében a kétpaneles pár
  és a Microsoft a forrás (Total Commander `Letöltés a háttérben`, Double Commander `Ha az alkalmazás a háttérben fut`,
  ms `háttérben futó feladat`). A macOS `hu` a szót KIZÁRÓLAG látvány értelemben ismeri (`Háttérkép`, `háttérszín`),
  tehát itt nincs Tier-1 döntőbíró, nem pedig hiányzik (2. bányászati csapda) · high.
- **Szórend: `A Cmdr legutóbb a háttérben problémába ütközött`** · a `.ended` testvér mintája
  (`A Cmdr legutóbb váratlanul bezárult`): `A Cmdr` + `legutóbb` + a módosító + az ige. A fókuszpozícióba (közvetlenül
  az ige elé) a `problémába` kerül, a `a háttérben` a topikmezőbe: ez a semleges magyar olvasat · high.
- **A második mondat szó szerint a `.ended`-é, az `összeomlási` jelző nélkül:
  `Itt egy jelentés a részletekkel, ami segíthet ezt kijavítani.`** · az angol is „a report”-ot mond „a crash report”
  helyett, mert semmi nem omlott össze. A diagnosztikai `jelentés` Tier 1 (`CrashReporterSupport`: `Jelentés…` gomb,
  `küldjön jelentést az Apple számára`) és Microsoft (`report` = `jelentés`) · high.
- **A két kulcson tiltott szavak** (a pile szerint ezek viszik a „leállt” jelentést): `váratlanul kilépett` (Apple
  összeomlás-párbeszéd), `váratlanul bezárult` (AppKit, és a `.ended` sajátja), `összeomlott` / `összeomlás` (ms, AppKit
  `Összeomlás` gomb). A puszta `leáll` és `kilép` a pile-ban végig szándékos megállítást jelöl, de ezek a kulcsok
  egyiket sem használják.
- **A `.unknown` a hátteret sem nevezi meg**: régi Cmdr-verzió jelentése áll mögötte, amely nem rögzítette, hogy az app
  tovább futott-e, ezért a mondatnak mindkét kimenetelre igaznak kell lennie.
- `sameAsSourceJustification` egyik kulcsnál sem kell: mindkét érték eltér az angoltól.
- A cím ugyanezt a hasítást követi: `crashReporter.dialog.title.crash` = `Elküldöd az összeomlási jelentést?` marad,
  `.title.report` = `Elküldöd a jelentést?` az a két eset, amelyik nem állíthat összeomlást. Ugyanez a művelet a
  visszajelző pirítósnál (`sentToast.message.crash` / `.message.report`): csak az `összeomlási` jelző esik ki.

## A jelentésküldés beállításszövege már mindkét kimenetelre igaz (`settings.updates.crashReports.description`)

A kapcsoló akkor is küld jelentést, ha egy háttérbeli panic NEM zárta be az appot, tehát a súgószöveg nem szólhat csak a
váratlan bezárulásról. Minden elem a fenti összeomlásjelentő-szakaszból jön, jelen időben:

- **`ha a Cmdr váratlanul bezárul`** a `crashReporter.dialog.body.ended` igéjéből (`váratlanul bezárult`, AppKit), a
  kulcs korábbi `váratlanul kilép` alakja helyett: a két felület így ugyanazt az igét mondja ugyanarra a kimenetelre ·
  high.
- **`a háttérben problémába ütközik`** a `.keptRunning`-ból, ugyanazzal a szórenddel (topik + módosító + fókusz + ige) ·
  high. A jelen idő puszta morfológia, nem új termdöntés.
- **`egy jelentést`** az `összeomlás-` jelző nélkül, mert a mondat mindkét esetre vonatkozik · high. ❌ A CÍMKE
  (`settings.updates.crashReports.label`) marad `Összeomlás-jelentések küldése`: az a beállítás neve.
- **A második mondat a `crashReporter.dialog.privacyNote`-ból jön** (`hogy a kód melyik része ütközött a problémába`),
  az `összeomlás helye` helyett, ami csak összeomláskor volt igaz · high.

- ⚠️ Angol oldali ellentmondás, NEM itt javítható: a párbeszéd címe mindhárom törzsváltozat alatt ugyanaz
  (`crashReporter.dialog.title` = „Send crash report?” = `Elküldöd az összeomlási jelentést?`), tehát a `keptRunning`
  ágon a cím összeomlást állít, miközben a törzs éppen ezt tagadja. A magyar hűen követi az angolt; a feloldás az `en`
  kulcs dolga.

## Kiadás és leválasztás: a `errors.eject.*` buboréküzenetek (2026-08-23)

Kilenc kulcs. Mind a KETTŐSPONT UTÁNI mondat egy rövid buborékban: a burkoló vagy `fileExplorer.pane.ejectFailedToast`
(`Nem sikerült kiadni: {volumeName}: {message}`), vagy `fileExplorer.pane.disconnectFailedToast`
(`Nem sikerült a leválasztás: {message}`). RAW család (nincs ICU), tehát egyszeres aposztróf; a `hu/errors.json`
egyetlen `''` párt sem tartalmaz, ez a fájl bevett alakja. Egy-két rövid mondat, markdown nélkül.

- **„in use” (a kötetet/meghajtót valami fogja) → `használatban van`** · macOS Finder Tier 1, sok találat: `NE66` („A
  kötet nem adható ki, mert jelenleg használatban van.”), `NE31`, `NE79`, `NE80`, `PE7`, `PE19` (ellenőrizve macOS
  26.5.2, `LocalizableMerged`, 2026-08-23) · high.
- **„Close any open files and apps, then eject again.” →
  `Zárd be a nyitott fájlokat és alkalmazásokat, majd add ki újra.`** · szerkezetében szó szerint a macOS `NE52` („Az
  ezeken a lemezeken lévő néhány fájl még használatban lehet. Léptessen ki minden nyitott alkalmazást, majd próbálja
  újra.”), önözésből tegezésbe téve · high. Az Apple `léptessen ki` (= quit) helyett `zárd be`, mert az angol is a
  lágyabb „close”-t mondja, és a mondat fájlokra is vonatkozik.
- **removable (meghajtóra) → `cserélhető`** · macOS Finder Tier 1 (`KIND_FORMATTER_28_0` = `Cserélhető kötet`,
  `KIND_FORMATTER_28_1` = `Cserélhető`, `GV3` = `Cserélhető kötetek`) ÉS Microsoft (`removable drive` =
  `cserélhető meghajtó`) · high. A két forrás egyetért, ezért nincs macOS-vs-Windows hasadás. Nem `eltávolítható`: arra
  egyik forrásban sincs fedezet.
- **network share → `hálózati megosztás`** · Microsoft-terminológia Tier 2 (`network share` = `hálózati megosztás`,
  HUN), a `megosztás` tő pedig már a szótár szava · high. A macOS a fogalmat `szerverkötet`-nek hívja (`FF22.2`), de az
  a csatolt kötetet nevezi meg, nem magát a megosztást, ezért itt a Microsoft-alak a pontosabb.
- **device (telefon, tablet, kamera kábelen) → `eszköz`** · macOS Finder Tier 1 (`PE5.1` = „A művelet nem hajtható
  végre, mert az eszköz eltűnt.”, `PE92`), Double Commander („külső eszközök (például okostelefonok)”) · high.
- **„Unplug it once it's idle.” → `Húzd ki, amikor már nincs használatban.`** · az `unplug`-ra a `hu` pile-ban nincs
  közvetlen találat (a macOS `Tartsa csatlakoztatva az eszközt` a legközelebbi, ellentétes irányból), a `kihúz` viszont
  a köznyelvi alak, és a második fele a fenti Tier-1 `használatban van` tagadása · high a `használatban` részre,
  `tentative` a `Húzd ki` igére.
- **`errors.eject.timedOut` NEM kudarcként fogalmaz**:
  `A meghajtó még nem válaszolt, de a kiadás magától is befejeződhet.` · a testvér `errors.mutation.timedOut` mintája
  (`A kötet még nem válaszolt, így a módosítás attól még végbemehet.`) és a `nem válaszolt` Tier-1 töve · high. A
  `-hat/-het` viszi az angol „may still eject on its own” engedékeny jelentését; a `kiadás` főnév kerüli a
  `kiadódik`-féle kényszeredett visszaható alakot.
- **`errors.eject.unexpected` SZÁNDÉKOSAN eltér a szó szerint azonos angolú `errors.mutation.unexpected`-tól.** Angol
  mindkettőnél: „Something went wrong, and Cmdr couldn't tell what.” (azonos `sourceHash`, `0c9d9f5`).
  - `errors.mutation.unexpected` marad `Valami nem sikerült, és a Cmdr nem tudta megállapítani, hogy mi.`
  - `errors.eject.unexpected` = `A Cmdr problémába ütközött, és nem tudta megállapítani, hogy mi.`
  - **Miért**: a kiadás-buborék burkolója maga `Nem sikerült kiadni: …`-val kezdődik, tehát a settled alak közvetlen
    szóismétlést adna („Nem sikerült kiadni: Naspolya: Valami nem sikerült, és…”). A `problémába ütközött` a fenti
    összeomlásjelentő-blokkban már bizonyított Tier-1 szerkezet (`NE105` = „A(z) „^0” hibába ütközött.”), a mondat
    második fele pedig szó szerint a settled alaké, így a két kulcs továbbra is egy hangon szól · high.
- **`errors.eject.busy`**: `A Cmdr még fájlokat mozgat ezen a meghajtón. Add ki, amint ez befejeződik.` A `Cmdr` alanyos
  szerkezet a katalógus bevett alakja (22 `A Cmdr nem …` érték); az `ezen a meghajtón` helyhatározós forma kerüli a
  toldalékolt helyettesítőt, ahogy a `style.md` § Notes and decisions előírja.
- Egyik kulcsnál sem kell `sameAsSourceJustification`: mind a kilenc érték eltér az angoltól.

### A macOS-panelnevek magyarul: `Infó megjelenítése` és `Zárolt` (ugyanez a passz)

A `errors.write.fileLocked.suggestion.mac` és a `errors.write.permissionDenied.suggestion.deleteMac` addig angolul
hagyta a két panelnevet („Get Info”, „Locked”) egyébként magyar mondatban. Az Apple MINDKETTŐT lefordítja, tehát a

1. terminológiai alapelv (fordítsd, amit az Apple fordít) szerint magyarul kell állniuk; a `BRAND_WORDS` sem tartalmazza
   egyiket sem.

- **Get Info → `Infó megjelenítése`** · macOS Finder Tier 1, közvetlen kulcs-egyeztetéssel: `Localizable`
  `"Get Info" = "Infó megjelenítése"`, továbbá `MenuBar.strings` `300801.title` (en_GB `Get Info` → hu
  `Infó megjelenítése`), `LocalizableMerged` `N165`, `TL22`, és futó szövegben `NE18`, `BN43`, `N30`, `PE14` („Válassza
  a Fájl > Infó megjelenítése parancsot…”). Ellenőrizve macOS 26.5.2 (`sw_vers`), 2026-08-23 · high.
- **Locked (a jelölőnégyzet az infóablakban) → `Zárolt`** · macOS Finder Tier 1: `InfoWindowGeneralView.strings`
  `1073.title` (en_GB `Locked` → hu `Zárolt`). Az Apple futó szövegben idézőjelbe teszi (`NE18`: „szüntesse meg a
  „Zárolt” kijelöltségét”, `NE43`, `PE14`), ezért a katalógusban is `„Zárolt”` a `style.md` idézőjel-szabálya szerint.
  Ellenőrizve macOS 26.5.2, 2026-08-23 · high.
- **A regiszter marad Cmdr-es, csak a CÍMKÉK az Apple-éi.** Az Apple mondata önöz és hivatalos („szüntesse meg a …
  kijelöltségét”); a miénk tegez és köznyelvi: `vedd ki a „Zárolt” pipát`. A menüutat az Apple sem teszi idézőjelbe, a
  jelölőnégyzet nevét igen; ezt követjük.
- **A két `errors.listing.*` kulcs is magyar** (`noPermissionErrno.suggestion`, `permissionDenied.suggestion`): a
  „válaszd a Get Info menüt, és nézd meg a Sharing & Permissions részt” helyén most
  `válaszd az Infó megjelenítése parancsot, és nézd meg a Megosztás és jogok részt` áll.
- **Sharing & Permissions → `Megosztás és jogok`** · macOS Finder Tier 1, kulcs-egyeztetéssel:
  `InfoWindowPermissionsView.strings` `6.title` (en_GB `Sharing & Permissions:` → hu `Megosztási jogok:`, ez a panel
  FEJLÉCE), futó szövegben pedig `LocalizableMerged` `N30`/`N32`/`NE43` (en „check the Sharing & Permissions section” →
  hu „kattintson a Megosztás és jogok részre”). Ellenőrizve macOS 26.5.2, 2026-08-24 · high. A futó szöveges alakot
  választjuk, mert a mi mondatunk is futó szöveg; a panel fejlécében az Apple maga is rövidít.
- **`parancsot`, nem `menüt`** · az Apple futó szövege a menüelemre `parancs`-ként hivatkozik („válassza a Fájl > Infó
  megjelenítése parancsot”); a korábbi „a Get Info menüt” tárgyilag is téves volt (menüelem, nem menü). A névelő `az`,
  mert az `Infó` magánhangzóval kezdődik.

## A Kuka-értesítés két gombja és a visszavonás családja (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, 2026-08-27)

Kilenc új kulcs: a Kukába helyezés után felbukkanó értesítés két gombja, a visszavonás folyamat- és eredményszövegei, és
a parancspaletta „Go to trash” parancsa.

- **undo (a gomb) → `Visszavonás`** · macOS Finder `ME13` Tier 1 (`Undo` = `Visszavonás`), és a katalógus már ezt
  szállítja ugyanerre az angol gombra (`askCmdr.renameUndo.undo`) · high. Egy szó, elfér a keskeny értesítésben.
- **put back (a művelet, amit a gomb elindít) → `visszahelyezés`** · macOS Finder `PE130_V1`/`PE130_V2` Tier 1 („^0
  items could not be put back.” = „^0 elem visszahelyezése nem sikerült.”) · high. A Finder MENÜPARANCSA `Visszatevés`
  (`N153.1`), tehát a tő közös; a mi szövegeink mondatok, nem menücímkék, ezért a mondatbeli alakot vesszük, ami
  ráadásul a szótár `move → áthelyezés` sorával is egy családba esik. NEM `visszaállítás`: azt az `askCmdr.renameUndo.*`
  a RÉGI NÉV visszaadására használja, más művelet.
- **„Go to trash” → `Ugrás a Kukába`** · macOS Finder `TL_HELP_TCAN` Tier 1 („Go to the Trash” = „Ugrás a Kukába”) ·
  high. Ugyanaz az érték a gombon és a parancspaletta címkéjén, ahogy az angolban is. A `Kuka` nagybetűs marad (settled,
  a funkció neve), ahogy a Finder is írja ebben a sorban.
- **„Putting them back...” → `Az elemek visszahelyezése…`** · főnévi folyamatalak, ahogy a fájl többi haladásjelzője
  (`transferDialog.checkingConflicts` = `Ütközések keresése…`); `elem` a settled item-szó. A `hu` katalógus ebben a
  fájlban `…`-t (U+2026) használ, nem három pontot, akkor is, ha az angol forrás `...`-ot ír.
- **„Put back N files.” → `{countText} … visszahelyezve.`** · pontosan a testvér `transfer.trash` alakja
  (`{countText} {count, plural, one {fájl} other {fájl}} áthelyezve a Kukába`), csak az igenév cserélődik. A számnév
  után a főnév EGYES SZÁMBAN marad mindkét ágban (a `style.md` § Plurals fő szabálya).
- **A részleges eredmény második fele kap egy főnevet.** Az angol forrás itt `item`-et mond, tehát a settled `elem` a
  szó: `{skippedText} {skipped, plural, one {elem} other {elem}} a Kukában maradt`. A `{skippedText}` mellett ott van a
  `{skipped}` egész számú társ is, de a magyarnak nincs rá szüksége: a számnév utáni főnév mindkét ágban egyes szám
  marad, csak az ICU kéri, hogy mindkettőt kiírjuk.
- **„Nothing to put back. …” →
  `Nincs mit visszahelyezni. Lehet, hogy ezek az elemek már a helyükön vannak, vagy a meghajtójuk nincs csatlakoztatva.`**
  · a testvér `askCmdr.renameUndo.unavailable` mondatszerkezetét viszi tovább
  (`Nincs mit visszaállítani. Lehet, hogy …, vagy a meghajtója nincs csatlakoztatva.`), csak a művelet szava más · high.
- **„This drive doesn't keep a trash.” → `Ezen a meghajtón nincs Kuka.`** · a már rögzített `Ezen a köteten nincs Kuka.`
  alak, `kötet` helyett `meghajtó`, mert az angol itt `drive`-ot mond · high. Tényközlés, nem hibaüzenet: ezért nem a
  `errors.write.trashNotSupported.message` („nem támogatja”) regisztere.
- **A parancs leírása → `Az aktuális meghajtó Kukájának megnyitása`** · főnévi leírásforma, ahogy a `commands.json`
  többi leírása (`Másolat készítése a kijelölt fájlokról ugyanabban a mappában`); az `aktuális` a katalógus szava a
  „current”-re (`commands.editPaste.description`, `commands.favoritesAdd.description`) · high.

## A már elküldött jelentés kiegészítése (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`, 2026-08-28)

Tizenegy új kulcs: a Cmdr magától elküldi a hibajelentést, az értesítés gombja pedig megnyit egy ablakot, ahol a
felhasználó megnézheti, mi ment el, és megjegyzést fűzhet UGYANAHHOZ a jelentéshez (nincs második feltöltés).

- **add to X → `Hozzáadás a(z) X-hoz/-hez`** · macOS Tier 1 (`Hozzáadás a Kedvencekhez`, `Hozzáadás a Dockhoz`,
  `Hozzáadás az oldalsávhoz`), Microsoft-terminológia (`add` = `Hozzáadás`) · high. Innen `amend.title` =
  `Hozzáadás a hibajelentésedhez` és `amend.submit` = `Hozzáadás a jelentéshez`. A címke/gomb párost szándékosan ugyanaz
  a viszony köti össze, mint a küldő ablak `dialog.title` (`Hibajelentés küldése`) és `dialog.send` (`Jelentés küldése`)
  párját: a CÍM a teljes `hibajelentés` szót viszi, a GOMB a rövid `jelentés`-t.
- **note (a felhasználó szabad szövege) → `megjegyzés`** · Microsoft-terminológia (`comment` = „A note or annotation
  that an author or reviewer adds to a document” = `megjegyzés`; a `note` szócikk hu oldalán is szerepel a
  `megjegyzés`), Double Commander (`Fájl/mappa megjegyzés`, `Megjegyzés szerk&esztése...`) · high. Megerősíti a már
  szállított `errorReporter.dialog.noteLabel` = `Megjegyzés hozzáadása (nem kötelező)`. NEM `jegyzet`: a Microsoft azt a
  külön álló Notes-elem értelmére tartja fenn (`Jegyzetek`, `Skype-jegyzetek`), a macOS pile-ban pedig a `megjegyzés`
  csak a „remember” jelentésben fordul elő (`Helyesírás megjegyzése`), ami itt nem zavaró, mert a mi mondataink tárgya
  mindig maga a szöveg.
- **view (a művelet) → `megtekintés`** · macOS Tier 1 (`A(z) %@ megtekintéséhez jelentkezzen be`,
  `Megosztott (csak megtekintés)`), Microsoft-terminológia (`view` = `megtekint`) · high.
- **„Your note” → `A megjegyzésed`** · a mezőcímkék határozott névelős birtokos alakja már a katalógus szokása
  (`settings` „Your email address” = `Az e-mail-címed`) · high. Az angol itt szándékosan elhagyja a „(optional)”-t, mi
  is elhagyjuk.
- **„What was sent” → `Mi került elküldésre`** · a testvér `errorReporter.dialog.detailsToggle` = `Mi kerül elküldésre`
  MÚLT IDEJŰ alakja, betű szerint ugyanaz a szerkezet. A két kapcsoló egymás mellett él ugyanabban a funkcióban, és az
  angol is csak igeidőben tér el („What''s about to be sent” / „What was sent”), ezért a magyar sem hoz be új
  szerkezetet. A `kerül + -ásra/-ésre` alak a fájl saját idiómája (`eltávolításra kerülnek`, `nem kerülnek elküldésre`).
- **„Adding…” → `Hozzáadás…`** · a testvér `dialog.sending` = `Küldés…` főnévi folyamatalakja ugyanezzel a lemmával ·
  high. U+2026, nem három pont.
- **„That report can''t take a note any more. …” →
  `Ehhez a jelentéshez már nem lehet megjegyzést hozzáadni. Ha el szeretnéd juttatni a megjegyzésedet a csapathoz, küldj új jelentést a Súgó menüből.`**
  · a `már nem` tagadás a macOS mintája (`… vagy Ön már nem rendelkezik engedéllyel …`); a `Súgó` a szótár szava (mac
  Tier 1, `menu.bar.help` = `Súgó`), a menüből-irányítás pedig a macOS „válassza az Apple menü > …” mondatainak tegező
  változata. A `hozzáadni` igét azért választottuk a szebb `megjegyzést fűzni` helyett, mert a `fűz` lemma a pile-ban
  csak az `összefűz`/`hozzáfűz` (append) jelentésben él, a `hozzáad` viszont az egész funkció settled igéje. Nincs benne
  sem `hiba`, sem `nem sikerült`: tényközlés, nem hibaüzenet.
- **„Couldn''t add your note: {error}” → `Nem sikerült hozzáadni a megjegyzésedet: {error}`** · pontosan a testvérek
  szerkezete (`Nem sikerült elküldeni a hibajelentést: {error}`, `Nem sikerült menteni a csomagot: {error}`) · high.
- **„Note added to your report. Your reference ID is” →
  `A megjegyzésed bekerült a jelentésbe. A hivatkozási azonosítód:`** · a testvér `sentToast.message`
  (`A hibajelentés elment. A hivatkozási azonosítód:`) második mondata változatlanul, kettősponttal, mert az azonosító
  közvetlenül utána jön egy jelvényben. A `hivatkozási azonosító` settled.
- **„View or add notes to the report” → `Megtekintés vagy megjegyzés hozzáadása`** · FLAGGED (lásd lent). Mindkét fele
  megvan (nézés + hozzáadás), főnévi címkeformában, és elfér az értesítés keskeny gombsorában a rövidebb
  `Beállítások módosítása` mellett. A „to the report” nem jelenik meg külön: fölötte ott áll az értesítés címe
  (`A hibajelentés elment`), tehát a tárgy egyértelmű, a teljes `Jelentés megtekintése vagy megjegyzés hozzáadása`
  viszont már 47 karakter lenne.

Natív anyanyelvi ellenőrzésre megjelölve:

- `errorReporter.autoSentToast.viewOrAddNotes`: a tömörség és a teljesség közti váltás megítélése (kimarad-e a
  „jelentés” szó a gombról) ízlés kérdése; a jelenlegi alak a tömörebbet választja.
- `errorReporter.amend.description`: a második tagmondat (`és odakerül a többi mellé, ami már a csapatnál van`) az angol
  „it''ll join what the team already has” szándékosan lezser képét viszi tovább; érthető és tegező, de több egyformán jó
  magyar megoldás létezik rá.

## Kijelölés és a kijelölés törlése: a Select / Deselect párbeszéd (`selection.*`, 2026-08-29)

- **select → `kijelölés`, deselect → `kijelölés törlése`** · macOS 26.6.2 Finder `hu`
  (`Finder.app/Contents/Resources/hu.lproj/MenuBar.strings`, `172.title` = `Összes kijelölése`, `300488.title` =
  `Kijelölés törlése`; ellenőrizve 2026-08-29) · high. Ez a natív menük szakaszának `Deselect all` döntését erősíti meg,
  és innen jön a párbeszéd két címe: `Fájlok kijelölése` / `Fájlok kijelölésének törlése`, szóról szóra a
  `menu.select.files` / `menu.select.deselectFiles` alakja, hogy a cím és az őt megnyitó menüpont ne mondjon mást.
- **A `megszüntetése` alak sehol nem maradt**: a `commands.selectionDeselectFiles.label` és a
  `commands.selectionDeselectAll.label` is a `törlése` alakot viszi, tehát a parancspaletta, a menüsor és a párbeszéd
  ugyanazt a szót mondja. A Tier-3 ortodox pár ugyan a `megszüntetése` szót használja (Total Commander `hu`
  „Csoportkijelölés megszüntetése”, `WCMD.INC` 522; Double Commander `hu` „Csoport kijelölésének megszüntetése”), de a
  Tier-1 Finder a `törlése`, és a Finder nyer. ❌ Ne told vissza a `megszüntetése` alakra.
- **A súgóbuboréknak NEM kell tartalmaznia a feliratot** (`selection.action.*`). A gomb hozzáférhető neve a felirat
  kulcsából jön (`QueryDialog.svelte`: `aria-label={config.primaryAction.ariaLabel ?? config.primaryAction.label}`), a
  súgóbuborék pedig egy belső `span` `use:tooltip` akciója, tehát a WCAG 2.5.3 már a felépítésből adódóan teljesül. A
  ház precedense ugyanezt mondja: a `search.action.showAll.label` (`Összes megjelenítése a főablakban`) és a `.tooltip`
  (`A találatok megnyitása az aktív panelen`) szándékosan más szavakat használ. A buborék tehát szabadon fogalmazható;
  csak ugyanazt a műveletet nevezze meg, mint a felirat, és mondja ki, hogy a fókuszált panelen történik.
- **Ettől függetlenül a felirat így is a buborék eleje lett**, mert a két legközelebbi testvér ezt a formát viszi:
  főnévi szerkezet + helyhatározó (`A találatok megnyitása az aktív panelen`, `A fájl megnyitása az aktív panelen`). Nem
  kényszerből, hanem mert ez a kulcscsalád háziformája.
- **A `fájlok kijelölésének törlése` alak nyelvtani döntés, nem a buborékért van**: az `itt látható fájlok` birtokos
  szerkezete EGY szintű (pont a `menu.select.deselectFiles` alakja), míg az
  `Ezeknek a fájloknak a kijelölésének a törlése` kétszintű birtokos lánc lenne. Ez a felirat önmagában is jobb,
  akárhogy szól majd a buborék.
- **focused pane → `a fókuszált panel`** · a katalógus szava (`commands.navGoToPath.description`,
  `commands.favoritesAdd.description`) · high. Helyhatározóban `a fókuszált panelen`. A `search.action.*.tooltip`
  `az aktív panelen` alakja az angol „active pane” párja, tehát nem ugyanaz a kulcscsalád.
- **`selection.runHint` a `search.runHint` mintája**: `Nyomd meg az Entert a szűréshez` (a testvér
  `Nyomd meg az Entert a kereséshez`). Az Enter billentyű neve `Enter`, tárgyesetben `az Entert`, ahogy a katalógus
  mindenütt írja.
- **`selection.recent.*` a `queryUi.recent.*` ikertestvére**, csak a „keresés” helyén `kijelölés` áll:
  `Az összes legutóbbi kijelölés megjelenítése`, `Összes legutóbbi kijelölés`, `Legutóbbi kijelölések szűrése`,
  `Nincs a szűrőnek megfelelő legutóbbi kijelölés.`, `Legutóbbi kijelölések` (a popover és a lista az angolban is
  szándékosan azonos). Az `applyAria` a `search.recent.runAria` szerkezetét viszi:
  `Legutóbbi {mode} kijelölés alkalmazása: {query}`. A `{query}` a felhasználó nyers szövege, ezért a mondat végén,
  kettőspont után áll, így bármi elfér benne.
- Mind a 15 érték eltér az angoltól, tehát nincs szükség `sameAsSourceJustification`-re. Egyik értékben sincs aposztróf,
  így az ICU `''` szabálya nem lép be; a `{mode}` és a `{query}` változatlan.

## Belső driftszedés: egy fogalom, egy név (2026-08-30)

A `desktop-i18n-term-consistency` 28 divergenciát talált a `hu` katalógusban (egy angol érték, két magyar alak).
Tizenhat valódi drift volt, tizenkettő szándékos határvonal. A drift két oka: (a) egy szót menet közben újradöntöttünk,
de csak a hívóhelyek egy részét írtuk át, és (b) a menüsoros passz csak a `menu.json`-t frissítette, így a
parancspaletta a régi szót vitte tovább. A határvonalakat ez a szakasz írja le, hogy a következő passz ne lapítsa el
őket.

### A javított driftek

- **Quit Cmdr → `Kilépés a Cmdrből`** · macOS AppKit `hu` (`Quit` = `Kilépés`) és Finder `hu` (`Kilépés a Finderből`) ·
  high. A `commands.appQuit.label` `Cmdr bezárása` alakja a `close`-t mondta a `quit` helyett.
- **zoom in / out → `Felnagyítás` / `Lekicsinyítés`** · Safari `hu` `MainMenu.strings` `438.title` / `439.title` (a
  telepített macOS 26.x-ből, ellenőrizve 2026-08-30) · high. A `commands.viewZoom*` a régi `Nagyítás` / `Kicsinyítés`
  alakot vitte, ami ütközött volna a zoom-almenü saját címével.
- **Connect to server → `Kapcsolódás szerverre`** · macOS Finder `hu` `N84` = `Kapcsolódás szerverre…`, `FR15` =
  `Kapcsolódás a szerverre` · high. ❗ A katalógus két kulcsa a nyelvtanilag „helyesebbnek” tűnő `szerverhez` alakot
  vitte; a Tier-1 Apple-szóhasználat a `-ra/-re`, és az nyer.
- **Connected → `Kapcsolódva`** · macOS AppKit `hu` `SavePanel` (`Connected` = `Kapcsolódva`) · high. A `Csatlakozva`
  alak három kulcsból eltűnt (`ai.cloud.connected`, `ai.cloud.connectedNoModels`,
  `fileExplorer.network.browser.status.connected`). A folyamatban lévő `Connecting…` marad `Csatlakozás…`, mert az Apple
  is ezt az igét használja rá (`SavePanel`).
- **Try again → `Próbáld újra`, Retrying → `Újrapróbálás`** · a katalógus tegező regisztere · high. A két fogalom külön
  alakot kap: a gomb felszólít, a folyamatjelző főnévvel nevez. Az `Újrapróbálkozás` alak megszűnt.
- **case-sensitive → `Kis- és nagybetűérzékeny`** · macOS `hu` (`Kis- és nagybetűérzékeny`, egybeírt összetétel) · high.
  A `Kis- és nagybetűre érzékeny` és a `Kis- és nagybetűk megkülönböztetése` alak is erre cserélődött, a
  `queryUi.scope.toggle.caseSensitiveAria` is (`… illesztés`), hogy a WCAG 2.5.3 tartalmazása megmaradjon.
- **Dismiss → `Elvetés`** mindenütt, a `viewer.reloadToast.dismissTooltip` `Eltüntetés` alakja is.
- **`settings.section.imageIndexing` → `Képek indexelése`**, mint a testvére, a `settings.section.driveIndexing`
  (`Meghajtó indexelése`) és az `indexing.enrich.label`.
- Egy-egy alakra hozva: `Go to home folder` → `Ugrás a saját mappára` (a paletta, a Ugrás menü és a hibapanel gombja),
  `Low disk space` → `Kevés lemezterület`, `On disk` → `Lemezen(:)`, `Example: {model}` → `Példa: {model}`, és a két
  béta-feliratkozós mondat (siker + kudarc) az onboarding alakjára.

### A határvonalak, amiket NEM szabad elsimítani

- **Cancel**: `Mégsem` a párbeszéd elvető gombja (macOS Finder), `Megszakítás` az, ami egy FUTÓ műveletet állít le
  (`queue.row.cancel`, `transferProgress.titleCancelling`, `errors.write.cancelled.*`, `operationLog.status.canceled`).
  A `Leállítás` a harmadik: egy szolgáltatást állít le (szerver, indexelés, Ask Cmdr).
- **View**: `Nézet` a menüsor NÉVSZÓI menücíme, `Megtekintés` az IGE, ami az F3 megjelenítőt nyitja (`menu.file.view`,
  `commands.fileView.label`). Az angol `@key` leírás is így különbözteti meg őket.
- **Zoom**: `Nagyítás` a szövegnagyítás (View almenü), `Méretezés` a Window > Zoom ablakművelet (macOS AppKit `hu`
  `Zoom All` = `Összes méretezése`). Az angol leírás külön kiemeli, hogy a kettő nem ugyanaz.
- **Select**: `Kijelölés` a fájlkijelölő menü és művelet, `Válassz` a legördülő lista helyőrzője
  (`ui.select.placeholder`) — ott a felhasználót szólítjuk meg, nem fájlt jelölünk ki.
- **Error**: `Probléma` a felhasználónak szóló állapotcella (az angol `@key` maga kéri a barátságosabb szót), `Hiba` a
  diagnosztikai előtag (`settings.updates.errorPrefix`, ahol az angol leírás kifejezetten megengedi).
- **Bytes**: `Bájtok` a folyamatsáv címkéje, mert a párja a `Fájlok`; `Bájt` a mértékegység-választó gombja, mert a
  szomszédjai `kB`, `MB`, `GB`.
- **From**: `Forrás` a `Cél` párja az átviteli párbeszéd fejlécében; `Innen:` a beágyazott útvonal előtti címke.
- **Purple**: `Bíbor` a Finder hét címkeszínének EGYIKE (macOS Finder `TG_COLOR_3`, szó szerint kell), `Lila` a Cmdr
  saját 12 színű kötetszínezőjében, ahol a köznyelvi színnév a helyes.
- **Put back**: `visszaállítva` a RÉGI NÉV visszaadása (`askCmdr.renameUndo.*`), `visszahelyezve` a Kukából való
  visszatétel (`fileOperations.trash.undone`). A `style.md` szótára már ezt írja elő; az angol mindkettőt „Put back”-nek
  mondja, ami az ANGOL pontatlansága, nem a miénk.
- **Rolling back**: `Visszagörgetés…` a folyamatablak címe (a három pont viszi a folyamatban-lévőséget),
  `Visszagörgetés folyamatban` az `operationLog` állapotcellája, ahol nincs három pont, és a szomszédai a KÉPESSÉGET
  nevezik meg (`Visszagörgethető`, `Nem görgethető vissza`) — ott a puszta főnév félreérthető lenne.
- **Send report**: `Elküldöd a jelentést` a párbeszéd CÍME (a felhasználót kérdezi), `Jelentés küldése` a GOMB.
- **`errors.eject.unexpected` ≠ `errors.mutation.unexpected`**: külön blokk indokolja fent (a kiadás-buborék burkolója
  már `Nem sikerült kiadni:`-val kezdődik, tehát a settled alak szóismétlést adna).
- **Az átnézés/átvizsgálás/keresés hármas**: `átnézés` az élő mappabejárás (`queryUi.results.live.*`,
  `search.walkHandoff.*`), `átvizsgálás` az index- és méret-átvizsgálás (`indexing.*`, `fileExplorer.dirSize.*`) meg az
  Ask Cmdr „végigmegy egy gyűjteményen” sorai, `keresés` maga a keresés funkció. Három fogalom, három szó.
- **memory**: `memória` a RAM (`ai.local.*`), `jegyzet` az Ask Cmdr emlékezete (`settings.askCmdr.memory.*`,
  `askCmdr.tool.memory*`).

## Az angol önellentmondásainak magyar utóélete (2026-08-30)

Az `en` katalógus öt helyen javította ki önmagát; itt az, amit ez magyarul eldöntött.

### A példa e-mail-cím: `te@example.com`

- **Helyi rész magyarul, domain `example.com`** · ms terminológia hu (`valaki@example.com`, `user@example.com`), RFC
  2606 · high. Mind a három mező ugyanezt viseli: `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`,
  `onboarding.stepBeta.emailPlaceholder`.
- `te@` a `te`-regiszterből jön (`style.md` § Formality), az angol `you@` közvetlen párja. A Microsoft hu terminológia a
  „someone” mintát viszi (`valaki@example.com`), tehát a helyi rész LEFORDÍTÁSA a bevett gyakorlat.
- ❌ NEM `pelda.hu` vagy `example.hu`: azok valóban regisztrálható domainek, tehát valakinek az igazi címe lehet. Az
  `example.com` az RFC 2606 példacélra fenntartott domainje, ezért soha nem lesz senkié.
- Ez felülírja a korábbi „you@example.com verbatim mindenhol” bejegyzést: az `en` `@key` most kifejezetten a helyi rész
  lokalizálását kéri.

### A régi NÉV visszaadása mondat most megnevezi a tárgyát

- `askCmdr.renameUndo.undone` / `.partial` →
  **`{countText} {count, plural, one {fájl} other {fájl}} régi neve visszaállítva.`** · a család már meglévő
  szóhasználata (`.undoing` = „A régi nevek visszaállítása…”, `.skipReason.failed.*` = „a régi nevét”) · high.
- Az angol korábban ugyanazt a „Put back {countText} {files}.” mondatot adta a NÉV-visszaállításnak és a Kukából való
  visszahelyezésnek; magyarul ez a kettő már addig is külön volt (`visszaállítva` vs `visszahelyezve`), most az angol is
  megnevezi a tárgyat. A magyar tárgymegnevezés a birtokos szerkezet (`… fájl régi neve`), mert számnév után a magyar se
  a főnevet, se a birtokot nem többesíti (`style.md` § Plurals).
- `fileOperations.trash.undone` változatlan: ott `visszahelyezve` a helyes, és marad.

### `mappa`, nem `könyvtár`, az indexelés súgószövegében

- `settings.indexing.enabled.description` → **`azonnali mappaméretekért`** (volt: `könyvtárméretekért`) · a glossary már
  előírja, hogy a `könyvtár` csak technikai értelemben járja, a UI szava a `mappa` · high. Az angol is „directory
  sizes”-ról „folder sizes”-ra váltott.

### A macOS-panelnevek magyarul, a futásidejű tokenek mellett

Nyolc `errors.*` kulcs angolul beégetett panelneveket hordozott; ezek most vagy futásidejű tokenek, vagy magyar
Apple-szóhasználat.

- `{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` **szó szerint marad**: a futásidőben a
  FELHASZNÁLÓ Mac-jén látható panelnevet kapja.
- ❌ **Soha ne ragassz esetragot, névelőt vagy toldalékot közvetlenül egy tokenre.** A régi „a System Settingsben” alak
  `{system_settings}ben`-t adna, ami a `Rendszerbeállítások` mellé rossz (a hangrend `-ban`-t kér). A katalógus máshol
  is használt `itt: {system_settings}` szerkezet a kiút (`style.md` § Agglutination).
- A tokenek által NEM fedett panelnevek magyarul mennek, ahogy az Apple írja őket:
  - **Apple Account → `Apple-fiók`** · macOS 26.6.2 (25G83),
    `AppleIDSettings.appex/Contents/Resources/InfoPlist.loctable` `hu.CFBundleDisplayName`, 2026-08-30 · high.
  - **General → `Általános`** · `hu/macOS/SystemSettings/Localizable.json` `GENERAL` · high.
  - **Login Items & Extensions → `Indítóelemek és bővítmények`** · macOS 26.6.2 (25G83),
    `LoginItems.appex/Contents/Resources/Localizable.loctable` `hu["Login Items & Extensions"]`, 2026-08-30 · high.

### A natív menüsor két Apple-tétele

`menu.app.showAll` / `menu.app.hideOthers` (és a párjuk, `commands.appShowAll.label` / `commands.appHideOthers.label`) →
**`Összes megjelenítése`** / **`Többi elrejtése`** · macOS 26.6.2 (25G83),
`Finder.app/Contents/Resources/hu.lproj/MenuBar.strings` `300730.title` / `300729.title`, 2026-08-30 · high. Az Apple
szóhasználata, a magyar mondatkezdő nagybetűvel (ami itt egybeesik az Apple alakjával). A `menu.*` család natív, ICU
nélkül renderelődik: aposztróf ott EGYSZER írandó.

## Egy félbehagyott visszagörgetés befejezése (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`, 2026-08-30)

- **`Finish rolling back` → `Visszagörgetés befejezése`** · macOS 26 Finder `hu`: `Másolás befejezése` (`NE108`, „Finish
  Copying”) és `Tömörítés befejezése` (`AR4`, „Finish Compressing”), tehát a `<főnév> befejezése` az Apple saját magyar
  alakja erre a műveletre (ellenőrizve 2026-08-30) · high. A `visszagörgetés` szótő a katalógusé
  (`operationLog.dialog.rollBack`, `rollingBack`, `partiallyRolledBack`), a nominális címkeforma pedig a `style.md` §
  Formality házirendje. A `befejezése` egyértelműen azt mondja, hogy VÉGIGVISZI a félbemaradt visszagörgetést, sosem
  azt, hogy újat indít.
- **A két `finishRollBack` kulcsnak betű szerint azonosnak kell maradnia.** Az `operationLog.dialog.finishRollBack` és a
  `fileOperations.rollbackConfirm.finishRollBack` angolja és művelete ugyanaz (`Finish rolling back`); ha a magyar
  értékük eltér, az `i18n-terms` figyelmeztet. A `Roll back` párnál a katalógus már így csinálja: `Visszagörgetés`
  mindkét helyen.
- **`Finish rolling this back?` → `Befejezed a művelet visszagörgetését?`** · a testvér
  `fileOperations.rollbackConfirm.title` („Visszagörgeted ezt a műveletet?”) regiszterét viszi: tegező kérdés, ugyanaz a
  `művelet` főnév · high. A mutató névmás szándékosan marad el: az `ennek a műveletnek a visszagörgetését` kétszintű
  birtokos lánc egy párbeszédcímben, az angol pedig maga is rövidít a testvéréhez képest (`Finish rolling this back?` a
  `Roll this operation back?` mellett). Egy modális ablakban úgyis egyetlen műveletről van szó.
- **A sor alatti magyarázó mondat a `fileOperations.rollbackConfirm.bodyUndoByDeleting` szavait viszi tovább**: „A Cmdr
  visszagörgette, amit tudott, a többit pedig úgy hagyta, ahogy volt. A befejezéshez még egy kör kell, és amiben a Cmdr
  továbbra sem biztos, azt megint kihagyja.” A `kihagyja` szóról szóra a testvéré, az `úgy hagyta, ahogy volt` pedig az
  `operationLog.rollback.refusalAlreadyRolledBack` („Ez már úgy van, ahogy korábban volt.”) és a
  `rollbackConfirm.leaveAsIs` („Maradjon így”) hangját tartja · high. A mondat szándékosan nem ígér teljes visszaállást:
  ami maradt, az lehet olyan fájl, amit a Cmdr nem tud a saját feljegyzéséhez kötni, és azt megint kihagyja.
- **Mérlegelés, nem forrás: `another pass` → `még egy kör`** · tentative. A pile egyik forrásában sincs erre alak, tehát
  ez saját döntés: a `még egy kör` köznyelvi és illik a tegező hanghoz. A `még egy átfutás` hivatalosabb, a
  `még egy menet` sportosabb; egyik sem jobban sourcolt. Ha egy későbbi menet jobbat talál, ez a hely cserélhető.
- **`in {folder}` → `a(z) {folder} mappában`: a ragot a KÖZNÉVRE tesszük, sosem a névre** · high. Tetszőleges
  mappanévhez nem lehet helyes toldalékot választani, mert a `-ban`/`-ben` illeszkedése és a névelő `a`/`az` alakja is a
  névtől függ. Ezért a `{folder}` jelzőként áll, a `-ban` rag pedig a `mappa` szóra kerül, ami mindig ugyanaz. Az `a(z)`
  a katalógus háziformája placeholder előtt (harminc körüli előfordulás, például
  `Eltávolítod a(z) {hostName} gépet a szerverlistából?`), és a macOS `hu` is így ír (`A(z) „^0”…`). A
  `queue.row.reversalDeleting` mellett a sor így szól: „A létrehozottak törlése a(z) Backup mappában” — a helyhatározó
  mondja ki, hogy a törlés a mappán BELÜL történik, különben a magában álló név úgy hat, mintha maga a mappa tűnne el.
  Ezt a hibát javítja a kulcs.
- Mind az öt érték eltér az angoltól, tehát nincs szükség `sameAsSourceJustification`-re. Egyetlen új értékben sincs
  aposztróf, így az ICU `''` szabálya nem lép be; a `{folder}` változatlan.

## A megszakított visszagörgetés eredményértesítése (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`, 2026-08-31)

Tizenhét új kulcs: a felhasználó `Visszagörgetés`-t nyomott egy futó másoláson vagy áthelyezésen, a visszacsinálás
lefutott, és ez az értesítés mondja el, mi sikerült belőle. Legfeljebb három rész, ebben az olvasási sorrendben: egy
címsor (`doneDeleting` / `doneMovingBack` / `someDeleted` / `someMovedBack` / `stoppedDeleting` / `stoppedMovingBack`,
mindig csak egy), a `leftBehind` bevezető sor, és alatta felsorolásban a `reason.*` indokok, mindegyik vagy MEGNEVEZI az
egy elemet (`*.named`), vagy MEGSZÁMOLJA őket (`*.counted`). Plusz a `rollbackConfirm.body`, aminek az angolja bővült.
Az egész hang: a Cmdr a gondos dolgot tette. Se bocsánatkérés, se riasztás.

- **"Left X alone: …" → a szállított `askCmdr.renameUndo.skipReason.*` keret: `<alany> változatlan maradt: <indok>.`** ·
  `high`, és két okból kötelező. (1) A `reason.folderNotEmpty.named`/`.counted` angolja BETŰ SZERINT azonos az
  `askCmdr.renameUndo.skipReason.folderNotEmpty.named`/`.counted` angoljával, tehát a `desktop-i18n-term-consistency`
  egyetlen magyar alakot vár rájuk, és a két család értéke betű szerint együtt mozog (lásd a névelő-szabályt lentebb).
  (2) Ha csak az a két sor igazodna, a felsorolásban négy `kimaradt` mellett állna egy `változatlan maradt`, amit a
  felhasználó EGY pillantással lát; a két funkció eltérése viszont sosem kerül egymás mellé. Az értesítés belső
  egyöntetűsége erősebb szempont, ezért a `drift` / `unverifiable` / `spotTaken` / `folderNotEmpty` mind a nyolc sora
  ugyanazt a keretet viszi.
  - **A `leftBehind` bevezető sora marad `kihagyja`**, szó szerint a testvér `rollbackConfirm.bodyUndoByDeleting`-ből
    (`Amiben a Cmdr nem biztos, azt kihagyja`) · high. A munkamegosztás így is megvan: a bevezető mondja ki az ÍGÉRETET
    és a következményt (`ezek a helyükön maradtak`), a sorok pedig elemenként az ÁLLAPOTOT (`változatlan maradt`).
  - **A `drift` sor látszólagos ellentmondása (`változatlan maradt: módosult…`) örökölt, és feloldható**: a
    `változatlan` a VISSZAGÖRGETÉSRE vonatkozik (a Cmdr nem nyúlt hozzá), a `módosult` pedig arra, ami korábban történt
    vele. Pontosan így él a szállított `askCmdr.renameUndo.skipReason.drift.named` is
    (`A {name} változatlan maradt: az átnevezés után módosult.`), tehát a keret ezt az olvasatot már elbírja.
  - ❌ NEM `békén hagyja` / `érintetlenül hagyja`: mindkettő értelmes magyar, de a `hu` pile egyikre sem ad egyetlen
    találatot sem.
- **A visszatétel igéje az EREDMÉNY-sorokban `visszahelyez`, a FOLYAMAT-sorokban marad `visszavitel`** · macOS Finder
  `PE130` Tier 1 (`^0 elem visszahelyezése nem sikerült.`), a testvér `fileOperations.trash.undone`
  (`{countText} fájl visszahelyezve.`), és az en `@key` kifejezetten ezt a testvért kéri · high. A katalógus
  folyamatszövegei ugyanennek a visszagörgetésnek a futása közben `visszavitel`-t mondanak
  (`transferProgress.titleReversalMovingBack` = `A fájl visszavitele…`, `queue.row.reversalMovingBack`,
  `rollbackConfirm.bodyUndoByMovingBack` = `Ez visszaviszi a fájlokat oda, ahonnan jöttek.`), és ez a kettősség
  MEGMARAD: a `visz` a mozgásra utal (miközben tart), a `helyez` a végállapotra (amikor megérkezett), és a magyar ezt a
  két aspektust külön szóval mondja. Gyakorlati bizonyíték is van rá: a `visszavisz` `-va/-ve` igeneve (`visszavíve`)
  egy értesítésben olvashatatlan, a `visszahelyezve` viszont pont a katalógus bevett eredményalakja.
  - Ezzel a „Put back” családnak három tagja van, mindegyik más művelet: `visszaállítva` = a RÉGI NÉV visszaadása
    (`askCmdr.renameUndo.*`), `visszahelyezve` = a Kukából és a visszagörgetésből való visszatétel
    (`fileOperations.trash.undone`, `cancelRollback.*`), `visszavitel` = a visszagörgetés futó folyamata. Lásd fentebb:
    § A Kuka-értesítés két gombja.
- **"Removed" → `eltávolítva`, sosem `törölve`** · a szótár `remove → eltávolítás` sora, és ugyanaz az érv, ami a
  kilépés-visszaszámláló "clears away" → `eltávolít` döntésénél: az értesítés megnyugtatás, nem szabad, hogy „a Cmdr
  fájlt töröl” villanjon fel benne · high. Az angol is szándékosan `Removed`-et mond, miközben a megerősítő párbeszéd
  `deletes`-t.
- **A „the N items” (teljes) kontra „N items” (részleges) szembeállítást a MONDATSZERKEZET hordozza, nem névelő** ·
  high. A `done*` pár `A Cmdr mindent eltávolított, amit létrehozott: {countText} elem.` /
  `A Cmdr mindent visszahelyezett: {countText} elem.` alakot kap (kimondott `mindent` = véglegesség, plusz a cselekvő
  megnevezése), a `some*` pár puszta igeneves számlálás: `{countText} elem eltávolítva.` /
  `{countText} elem visszahelyezve.`
  - ❌ NEM `Mind a(z) {countText} elem eltávolítva`: `count = 1` esetén `Mind az 1 elem …` lesz belőle, ami nem magyar.
    A `mind a(z)` + `{countText}` szerkezet minden ilyen kulcsban ez a csapda; a `mindent` + kettőspontos szám elkerüli.
  - A `done*` sorok megtartják a minősítést (`amit létrehozott`): a puszta `A Cmdr mindent eltávolított.` ijesztő, mert
    nem mondja meg, MIT.
- **"Stopped after …ing" → `{countText} elem <művelet>e után leállítva.`** · a `leállítás` a szótár stop-szava
  (`transferProgress.rollbackTooltip` = `Leállítás, és minden eddig kiírt fájl törlése`), a birtokos igenévi szerkezet
  pedig a macOS `^0 elem visszahelyezése` mintája · high. `The rest are still there.` → `A többi ott maradt.`;
  `The rest stayed where the move put them.` → `A többi ott maradt, ahová az áthelyezés vitte.` (`áthelyezés` = a szótár
  `move` szava).
- **"it changed" → `módosult`** · macOS Tier 1 (`A(z) „%@” fájl nem módosult a közelmúltban.`) · high. NEM
  `megváltozott`: a pile-ban a fájlra vonatkozó alak a `módosul`.
- **"Cmdr couldn''t check whether …" → `a Cmdr nem tudta ellenőrizni, hogy …`** · az `ellenőriz` a katalógus szava
  (`transferProgress.scanTitleCopy` = `Ellenőrzés a másolás előtt…`), a `nem sikerült`/`nem tudta` a nyugodt hangnem
  bevett alakja · high. Szándékosan NEM a `Nem sikerült megerősíteni, hogy …` család
  (`fileOperations.mkdir.timeoutMessage`): az angol itt `check`-et mond, nem `confirm`-ot, és a két fogalom külön él a
  katalógusban.
- **"something else now sits where it came from" → `már valami más van ott, ahonnan jött.`** · a `valami más` a testvér
  `transferProgress.foregroundBusyToast` szava (`Itt valami más van nyitva.`), az `ott, ahonnan jött` pedig szó szerint
  a `rollbackConfirm.bodyUndoByMovingBack` fordulata (`oda, ahonnan jöttek`) · high.
- **"Couldn''t undo {name}" → `A(z) „{name}” visszagörgetése nem sikerült.`** · macOS Finder `PE130` Tier 1 a
  mondatformára (`A(z) „^1” visszahelyezése nem sikerült.` / `^0 elem visszahelyezése nem sikerült.`) · high. Az angol
  itt a köznyelvibb `undo`-t mondja, a magyar mégis a szótár `visszagörgetés` szavát viszi: a katalógus per-ELEM
  kimenetele már `Visszagörgetve` (`operationLog.outcome.rolledBack`), tehát az egy elemre vonatkozó visszagörgetés már
  bevett, a `visszavonás` pedig a katalógusban egy MŰVELETRE vonatkozik, nem egy fájlra. A második mondat a testvér
  `fileOperations.trash.undoUnavailable` szerkezetét viszi (`Lehet, hogy … a meghajtójuk nincs csatlakoztatva`), a
  `csak olvasható` pedig a szótár szava, macOS Tier 1 (`egy csak olvasható köteten van`).
- **A `named` és a `counted` ág UGYANAZT a szerkezetet viszi, csak az alany más** · high. A `{countText} elem` alany
  magyarul EGYES számban egyeztet, ezért az állítmány mindkét ágban `változatlan maradt`; a kettőspont utáni indoklás
  viszont TÖBBES számú igét kaphat a számláló ágban (`… változatlan maradt: módosultak, …`), mert ott már a halmazra
  utalunk vissza. Ez nem lazaság: pontosan ezt csinálja a szállított `askCmdr.renameUndo.skipReason.drift.counted`
  (`… változatlan maradt: az átnevezés után módosultak.`). Így a számláló ágnak nincs szüksége kitett `ezek` névmásra
  sem.
- **Mérlegelés, nem forrás: `put it there` → `odatette`** · tentative. A pile egyik forrásában sincs erre alak. Az
  `odatesz` azért nyert, mert MÁSOLÁSRA és ÁTHELYEZÉSRE is igaz (az `odamásolta` csak az egyikre), és a `kiírta`
  (`transferProgress.rollbackTooltip` `kiírt fájl`) mappára nem áll, az `item` pedig itt mappát is jelent.
- **Névelő + `{name}` MINDIG `A(z) „{name}”`** · macOS Tier 1 (`A(z) „^0” elemet…`,
  `A(z) „^1” visszahelyezése nem sikerült.`) és a `hu` katalógus 22 olyan kulcsa, ahol névelő áll egy név előtt · high.
  A névelő `a`/`az` alakja a név ELSŐ HANGJÁN múlik, amit írás közben senki nem tud (`alma.txt` → `az`, `beszámoló.pdf`
  → `a`), tehát a puszta `A {name}` minden magánhangzóval kezdődő fájlnévnél hibás magyar. Az idézőjel ugyanabból a
  forrásból jön, és a hosszú vagy szóközös neveket is elhatárolja.
  - **A névelő NÉLKÜLI helyeket nem érinti**: kettőspont vagy birtokos szerkezet után a placeholder csupaszon marad
    (`Letöltve: {fileName}`, `{name} megnyitása`), mert ott nincs mit egyeztetni.
  - **A `{name}` továbbra sem kap RAGOT.** Ahol az angol köznevet is mond (`the folder {name}`), a köznév áll utána, és
    az visel minden ragot: `A(z) „{name}” mappa változatlan maradt`. Ugyanaz az elv, mint a `queue.row.reversalInFolder`
    `a(z) {folder} mappában` sorában.
  - **Ehhez KÉT család mozdult együtt** (2026-08-31): a négy új `cancelRollback.reason.*.named` sor, és a szállított
    `askCmdr.renameUndo.skipReason.*.named` mind az öt sora (`drift`, `nameTaken`, `unverifiable`, `folderNotEmpty`,
    `failed`), ami addig `A {name}`-et írt. Együtt kellett menniük, mert a `folderNotEmpty` pár angolja betű szerint
    azonos, tehát a `desktop-i18n-term-consistency` egyetlen magyar alakot vár rájuk; és mert egy félig javított család
    rosszabb bármelyik végállapotnál (a rename-undo értesítésben is egyszerre látszanak a sorok). Az öt szállított kulcs
    ANGOLJA nem változott, tehát a `sourceHash`-ük érintetlen: ez fordítási minőségjavítás, nem újrafordítás.
  - **Nyitott, más családokban**: az `errors.provider.appBased.transient`/`.needsAction`/`.serious` ugyanezt hozza egy
    szolgáltatónévre (`Ezt a mappát a **{name}** kezeli`, ahol a `{name}` iCloud/Dropbox/pCloud), az
    `askCmdr.renameUndo.undoJob` pedig egy SZÁMRA (`Mind a {countText} csomag…`: az `1` és az `5` más névelőt kér).
    Egyik sem ennek a két családnak a része, ezért ez a menet nem nyúlt hozzájuk; a szabály viszont rájuk is áll.
- **`rollbackConfirm.body`**: az angol egy harmadik mondattal bővült, ami betű szerint azonos a `bodyUndoByDeleting`
  záró mondatával (`Cmdr skips anything it isn''t sure about, so a few may stay behind.`), ezért a magyar is szó szerint
  annak a farkát veszi át (`Amiben a Cmdr nem biztos, azt kihagyja, szóval maradhat belőlük egy-kettő.`). Az első két
  mondat változatlan marad. Így a négy `rollbackConfirm` törzsszöveg egyetlen ígéretet mond, egyetlen megfogalmazásban.
- Mind a tizennyolc érték eltér az angoltól, tehát nincs szükség `sameAsSourceJustification`-re. Egyetlen új értékben
  sincs aposztróf, így az ICU `''` szabálya nem lép be; a `{name}`, `{countText}` és `{count}` mind változatlan.
## A túl régi WebKit blokkoló képernyője (`main.oldWebkit.*`, 2026-09-02)

Három szöveg, amit a Cmdr a felülete helyett mutat, ha a Mac Safarija túl régi. A HTML-vázban élnek, nem az appban,
tehát ez az egyetlen, amit az illető a Cmdrből lát.

- **`Software Update` → `Szoftverfrissítés`** · a macOS így nevezi a Rendszerbeállítások paneljét; a Finder Tier-1
  nyoma megerősíti a szót (`Apple Device Software Update File` → `Apple-eszköz szoftverfrissítési fájlja`) · `high`.
- **`Quit` → `Kilépés`** · macOS AppKit `Quit` → `Kilépés` · `high`. Eddig hiányzott a glosszáriumból, most bekerül.
- **A márka kötőjel nélkül toldalékolódik: `A Cmdrnek`**, a `style.md` § Brand and do-not-translate szabálya szerint.
- **`Safari 15.4-es vagy újabb verzió`**: a verziószám számjegy marad, a magyar toldalék kötőjellel kapcsolódik hozzá.
- **`Mac` marad `Mac`, ragozva `Macen`.** A `Safari` mostantól a `BRAND_WORDS` listán van.
