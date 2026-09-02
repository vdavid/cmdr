# Hungarian (hu) translation style guide

Working notes for translating Cmdr into Hungarian. Read `../README.md` for how this fits the translation process, and
the app-wide `docs/style-guide.md` for the English voice these notes carry into Hungarian.

macOS DOES ship a Hungarian UI localization (Finder, AppKit, System Settings), so it's the highest-authority source
here, exactly as on other languages. Microsoft Windows Hungarian plus the Microsoft Hungarian style guide are Tier 2,
and the GNOME Nautilus and Xfce Thunar file-manager catalogs are Tier 3 (cross-language parity). Glossary entries below
cite which sources back each choice.

## Voice and tone

Friendly, concise, active, calm. Hungarian software leans on a nominal style for labels (a button is a noun, "Másolás" =
"Copying", not a command), which reads clean and native rather than cold. Conversational copy stays warm and uses the
informal `te` address (see Formality). Error messages stay calm and actionable and never use "hiba" (error) or
"sikertelen" (failed) as a bare label: state the problem and a next step ("Nem sikerült átnevezni a fájlt. Megpróbálod
újra?").

## Formality

**Verdict: informal `te` (tegezés) throughout. No önözés.** Consumer brands (IKEA, Spotify, Netflix, H&M, Coca-Cola) all
address Hungarian users with `te`, which fits Cmdr's friendly personal voice. The OS sources lean önözés, but Cmdr
deliberately picks the warmer consumer-brand register. Formality decision recorded in `../formal-informal-decisions.md`.

- **Labels (buttons, menus, headers): nominal / infinitive, no direct address.** "Másolás", "Áthelyezés", "Törlés",
  "Mégsem". The dominant Hungarian UI convention; macOS Finder, Microsoft, GNOME, and Xfce all do this, and it sits fine
  under a `te` register since a label isn't direct address.
- **Conversational copy and questions: `te` (tegezés).** Where English addresses the user directly, use the informal
  second person. A friendly question is "Megpróbálod újra?", not the önözés "Megpróbálja újra?".
- **Cancel is "Mégsem"** (the macOS Finder button label), not "Mégse" or "Visszavonás" (that's undo). See the glossary
  note: this is a real macOS-vs-Windows split and macOS wins here.

## Terminology and glossary

Format: each line is `English: chosen · sources · confidence`. Confidence is `confirmed` (a human signed off), `high`
(authoritative sources agree), or `tentative` (sources conflict or none had it). Sources: mac = macOS Finder/AppKit, ms
= Microsoft terminology/style guide, gn = GNOME Nautilus, xf = Xfce Thunar. Contested terms get a short block.

- pane: `panel` · Double Commander hu ("Bal panel", "Jobb panel"), Total Commander hu ("a célpanelben") · high. The two
  file lists. There is no Tier-1 source (macOS Finder is single-pane), but the orthodox two-pane pair is Cmdr's own UI
  family and both members agree, which settles it; Microsoft's literal "ablaktábla" is the Windows term and stays out.
- tab: `lap` · mac ("Új lap"), ms ("lap") · high. "fül" is the colloquial alternative; "lap" is the macOS/MS standard.
- volume: `kötet` · mac ("Kötet"), ms · high.
- drive: `meghajtó` · mac, ms · high.
- folder: `mappa` · mac, ms, gn, xf · high.
- directory: `könyvtár` · mac (Localizable: "…könyvtárban"), ms · high. Technical sense only; prefer `mappa` in UI copy.
- file: `fájl` · mac, ms, gn, xf · high. Stays singular after a numeral ("3 fájl"). See Plurals.
- listing: `fájllista` · no direct source · tentative. The file list in a pane; descriptive compound, reads naturally.
- transfer: `átvitel` · mac, ms · high.
- delete (permanent): `törlés` · mac, ms, gn, xf · high.
- move: `áthelyezés` · mac, ms · high.
- copy: `másolás` · mac ("Másolás"), ms, gn, xf · high.
- rename: `átnevezés` · mac ("Átnevezés"), ms · high.
- viewer (the file viewer): `megjelenítő` · no exact Tier-1 match · tentative. macOS uses `Előnézet`/`Gyorsnézet` for
  preview, but those name Quick Look (a brand, kept verbatim). For Cmdr's own viewer, `megjelenítő` reads naturally.
- eject: `kiadás` · mac ("Kiadás", "Egy kiadása", "Összes kiadása") · high. "Lemez kiadása".
- disconnect (network): `leválasztás` · mac ("Leválaszt", "Kapcsolat bontása"), xf ("Failed to unmount" = "leválasztása
  sikertelen") · high.
- share (an SMB share): `megosztás` · mac, ms, gn · high.
- network share: `hálózati megosztás` · ms ("network share" = "hálózati megosztás", HUN) · high.
- removable (of a drive): `cserélhető` · mac ("Cserélhető kötet", "Cserélhető"), ms ("removable drive" = "cserélhető
  meghajtó") · high.
- device (a phone, tablet, or camera on a cable): `eszköz` · mac ("mert az eszköz eltűnt"), Double Commander ("külső
  eszközök (például okostelefonok)") · high.
- in use (something still holds the volume): `használatban van` · mac ("A kötet nem adható ki, mert jelenleg
  használatban van.") · high.
- Get Info (the Finder info window/command): `Infó megjelenítése` · mac (Finder `Localizable`, `MenuBar` `300801.title`)
  · high. Apple localizes it, so it is NOT a kept-English brand.
- Locked (the checkbox in that window): `Zárolt` · mac (Finder `InfoWindowGeneralView` `1073.title`) · high. Quote it in
  running text (`„Zárolt”`), as Apple does.
- search: `keresés` · mac ("Keresés"), ms, gn, xf · high.
- sort: `rendezés` · mac ("Rendezés módja"), ms · high.
- settings: `beállítások` · mac ("Beállítások"), ms · high.
- download: `letöltés` · mac, ms · high.
- index / indexing: `index` / `indexelés` · ms ("index") · high.
- overwrite: `felülírás` · mac ("Felülír"), ms, xf ("Felülírja?") · high.
- undo: `visszavonás` · mac (Finder `ME13` „Visszavonás”), a katalógus `askCmdr.renameUndo.undo` · high.
- put back (elem visszatétele a Kukából): `visszahelyezés` · mac (Finder `PE130` „…visszahelyezése nem sikerült”) ·
  high. A Finder menüparancsa `Visszatevés`, a mondatbeli alak `visszahelyezés`; mi az utóbbit használjuk, mert a
  szövegeink mondatok. NEM `visszaállítás` (az a régi NÉV visszaadása, `askCmdr.renameUndo.*`).
- go to trash: `Ugrás a Kukába` · mac (Finder `TL_HELP_TCAN` „Go to the Trash” = „Ugrás a Kukába”) · high.

Contested or split, with the per-source evidence:

### trash → `Kuka`

- mac: `Kuka` (30 occurrences), zero `Lomtár`.
- ms: gives both `kuka` and `lomtár`, but reserves `Lomtár` specifically for the Windows "Recycle Bin" product name.
- gn: `Kuka` ("Kukába dobva", "\_Kuka ürítése").
- xf: `Kuka` ("Áthelyezés a K_ukába", "Az összes fájl és mappa törlése a Kukából").
- Chosen: `Kuka` · sources mac, gn, xf (ms agrees as common noun) · high. This corrects the earlier "confirm Kuka vs
  Lomtár" open item: `Kuka` is what every Hungarian platform calls it; `Lomtár` is a Windows-product-name artifact.

### move to trash → `Áthelyezés a Kukába`

- mac: both `Áthelyezés a Kukába` and `Kukába helyezés`.
- xf: `Áthelyezés a Kukába`.
- Chosen: `Áthelyezés a Kukába` (nominal label style) · sources mac, xf · high.

### server → `szerver`

- mac: `szerver` (38 occurrences, e.g. "Kapcsolódás szerverre…"), with capitalized `Szerver` a few times.
- ms: `kiszolgáló` (terminology, HUN).
- gn/xf: a file manager rarely surfaces the term; `kiszolgáló` where present.
- Chosen: `szerver` · source mac (Tier 1) · high. A real macOS-vs-Windows split: Microsoft prefers `kiszolgáló`, but
  Cmdr is a macOS app and Finder users see `szerver`. This resolves the earlier open item in favor of `szerver`.

### bookmark → `könyvjelző`

- mac: `Kedvenc` (26x) names the Favorites sidebar; literal `könyvjelző` appears 3x.
- ms: `kedvenc`.
- gn: `könyvjelző` ("Hozzáadás a könyvjelzőkhöz", "Eltávolítás a könyvjelzőkből").
- Chosen: `könyvjelző` · source gn, plus mac's literal usage · tentative. macOS/MS `Kedvenc` names a Favorites _sidebar
  concept_, not an explicit bookmark action; for Cmdr's named bookmark feature the file-manager-native `könyvjelző`
  (GNOME) is clearer. Stays tentative — a macOS(`kedvenc`)-vs-GNOME(`könyvjelző`) split the next pass settles from the
  file-manager sources, not a call to park for David (see Open terms below).

### cancel → `Mégsem`

- mac: `Mégsem` (52 occurrences, the actual button label), zero `Mégse`.
- ms: `Mégse` (terminology and style-guide examples).
- gn/xf: `Mégse` ("\_Mégse", "Mé_gse").
- Chosen: `Mégsem` · source mac (Tier 1) · high. A genuine macOS-vs-Windows/Linux split. The earlier draft asserted
  `Mégse` and explicitly rejected `Mégsem`; macOS Finder, the highest authority and what the user sees, uses `Mégsem`,
  so Cmdr follows macOS. Never "Visszavonás" (undo).

Add lines as terms come up, keeping the `chosen · sources · confidence` format.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`).

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('hu')`; matches the GNOME/Xfce catalogs'
`nplurals=2; plural=(n != 1)`). You must still write both branches because ICU requires them, but mind the grammar:

- **Hungarian does NOT pluralize a noun after a number.** "1 fájl" AND "3 fájl", never "3 fájlok". The counted noun is
  singular in both the `one` and `other` branches; the branches differ only in any other agreement, not in the noun
  ending. Confirmed in the GNOME Nautilus catalog, where a counted-files entry keeps the singular noun in both plural
  slots (`msgstr[0]` = "%'d mappa kijelölve" AND `msgstr[1]` = "%'d mappa kijelölve", never "mappák"). This is the
  single biggest plural gotcha for Hungarian.
- No grammatical gender, which removes a whole class of agreement problems.
- **A numeral subject takes a SINGULAR predicate; a later clause may go plural.** `3 elem változatlan maradt`, never
  `3 elem változatlanok maradtak`. Once the sentence moves past the colon (or into a second clause) it refers to the
  set, so a plural verb is fine and idiomatic there: `3 elem változatlan maradt: módosultak, …`. Shipped example:
  `askCmdr.renameUndo.skipReason.drift.counted`.

## Notes and decisions

- **A natív menük a Finder szóhasználatát követik, nem a katalógusét.** Ahol a macOS-nak van megfelelője, az nyer
  (`Nézet`, `Saját`, `Kijelölés törlése`, `Méretezés`), mert a felhasználó a Cmdr menüsorát közvetlenül a Finderé
  mellett látja. Bizonyítékok és kivételek: `glossary.md` § Natív menük.
- **Agglutination + vowel harmony makes suffixed placeholders dangerous.** Hungarian attaches case suffixes that must
  harmonize with the word's vowels (`-ban`/`-ben`, `-ról`/`-ről`, `-hoz`/`-hez`/`-höz`) and sometimes double a final
  consonant. A `{path}` or `{name}` whose value is unknown can't take a correct suffix ("{path}-ban" may be wrong).
  Restructure so a placeholder isn't suffixed: put it after a postposition or in a neutral slot ("itt: {path}", not
  "{path}-ban").
- **Definite vs indefinite conjugation and the `a`/`az` article** depend on the following word, so phrasing around a
  placeholder needs care; prefer constructions that don't hinge on the inserted value's first sound. **When an article
  genuinely has to precede a name placeholder, write `A(z) „{name}”`** — the `a(z)` house form plus `„…”` quotes, both
  macOS Tier 1 (`A(z) „^0” elemet…`) and the catalog's majority. ❌ Never a bare `A {name}`: it renders "A alma.txt" on
  every vowel-initial name. Nothing is needed after a colon or in a possessive (`Letöltve: {fileName}`). Evidence and
  the families that were corrected to it: `glossary.md` § A megszakított visszagörgetés eredményértesítése.
  - **Quotes only around a NAME the user typed or owns.** A brand or provider placeholder takes bare `a(z) {name}`
    (`a(z) **{name}** kezeli`): the bold or the sentence already delimits it, and `„Dropbox”` reads as scare quotes.
  - **A placeholder with ONE possible value gets the real article, never the hedge.** `errors.provider.iCloud.*`'s
    `{name}` is always `iCloud Drive`, so it's `az **{name}**`. The `a(z)` form answers an UNKNOWN first sound; where
    nothing is unknown it's just noise.
  - **Read the whole string: one key often has two or three article sites.** The `errors.provider.appBased.*` lines
    carry `a(z) **{name}**`, `a(z) {app} appot`, and `a(z) {name} állapotoldalát`. Fixing the first and moving on leaves
    a half-corrected family, which is worse than either end state.
  - **In front of a NUMBER the article varies too, so ❌ never a bare `a {countText}`.** It follows the numeral's
    pronunciation: `a három`, `a négy`, but `az öt`, `a száz` but `az ezer`. In running prose the hedge is the answer
    and two shipped keys use it (`fileExplorer.imageIndex.folder.allIndexed`, `ui.loadingIcon.finalizing`); don't sweep
    those.
  - **For the phrase "all N X" specifically, prefer `Az összes X ({N})` over `Mind a(z) N X`.** macOS Hungarian words it
    that way (`Az összes lemez (^0) kiadásához…`), and it's strictly better: the article now agrees with `összes`, a
    word we choose, so nothing hinges on the runtime value at all. Worth the swap wherever the count can move to a
    parenthetical or behind a colon, and near-mandatory in a short button, where the hedge is most visible. Worked case:
    `glossary.md` § `askCmdr.renameUndo.undoJob`.
- **Sentence case is native** (Hungarian doesn't capitalize common nouns, days, or months), so the app's sentence-case
  rule applies cleanly. Don't capitalize the word after a colon unless it's a proper noun.
- **Suffix the brand WITHOUT a hyphen: `Cmdrt`, `Cmdrben`, `Cmdrrel`, `Cmdrnek`.** `Cmdr` is pronounced "commander", so
  its final written `r` does spell its final pronounced sound, and AkH's hyphen rule (silent final letter or an unusual
  letter cluster spelling the last sound) doesn't apply. Vowel harmony keys off the spoken form, so the back-vowel
  suffixes are the right ones. Same for the multiword product name: `Ask Cmdrt`.
- **Quotation marks: `„…”`** (low opening, high closing) is the standard Hungarian form. macOS Finder uses it too (e.g.
  „^0”). Avoid English `"…"`.
- **`{duration}` is NOT locale-formatted**, unlike numbers, sizes, and dates: `formatDuration()` in
  `apps/desktop/src/lib/units/duration.ts` always emits digits plus Latin unit letters (`45s`, `2m 30s`, `1h 5m`). So a
  duration placeholder can never take a Hungarian suffix (there's no reliable harmony for it, and the abbreviation isn't
  a Hungarian word). Put it in front of a postposition (`{duration} óta`, `{duration} van hátra`), never `{duration}-e`.
- **Multipliers spell the number out, no digits: `négyszer`, `százszor`, never `4x` or `4-szer`.** The Microsoft
  Hungarian style guide § 4.1.10 says numbers are written out when a suffix is attached to them (its own examples:
  `tízféle`, `kéthetente`), and the multiplier `-szor`/`-szer`/`-ször` is exactly such a suffix. The pile agrees: every
  multiplier in it is a word (`kétszer`, `háromszor`, `többször`, `háromszoros`), and there is not one digit+suffix
  form. Applies to speed comparisons in copy (`négyszer lassabb`), not to formatter output.
- **Numbers and dates come from the formatter layer.** Hungarian uses a comma decimal and space thousands separator, and
  a native `YYYY. MM. DD.` date order; `formatNumber()`/`formatByteSize()`/the date formatters produce these from the
  locale. Never hardcode separators or date order in a string.
- **Case suffixes are what break aria containment in Hungarian** (the shared rule: `../../guides/i18n-translation.md` §
  An `*Aria` key must contain its visible label). Take the case form the aria sentence already uses: `Háttérben` ⊂
  `Hagyd futni a háttérben`, `Sorba` ⊂ `Áthelyezés a műveleti sorba`. A capital mid-sentence isn't Hungarian, so
  containment here is always case-insensitive.
- **Length** runs near English; still overflow-check against the pseudolocale (`en-XA`).
- **A magyarázó prózában a nem végzetes probléma szava `probléma`**, nem `hiba` (a hiba-regisztert a hang kerüli) és nem
  `gond` (arra a `hu` pile nulla találatot ad). Forrás és teljes érvelés: `glossary.md` § Ha a Cmdr nem állt le.
- **A macOS panelneveit magyarul írjuk, mert az Apple is lefordítja őket.** `Get Info` → `Infó megjelenítése`, `Locked`
  → `Zárolt`, `Sharing & Permissions` → `Megosztás és jogok`. Egyik sincs a `BRAND_WORDS` listán, tehát az
  1. terminológiai alapelv (fordítsd, amit az Apple fordít) érvényes rájuk. A CÍMKÉK az Apple-éi, a MONDAT a miénk:
     tegezünk és köznyelvi maradunk (`vedd ki a „Zárolt” pipát`), nem másoljuk az Apple önöző hivatalnyelvét
     (`szüntesse meg a … kijelöltségét`). Bizonyítékok: `glossary.md` § A macOS-panelnevek magyarul.
- **Ugyanaz az angol mondat KÉT különböző magyar alakot kaphat, ha a burkoló szöveg eltér.** A `errors.eject.unexpected`
  és a `errors.mutation.unexpected` angolul betű szerint azonos, magyarul mégsem az: az előbbi a
  `Nem sikerült kiadni: …` burkoló után áll, ahol a settled `Valami nem sikerült` közvetlen szóismétlés lenne. Ilyenkor
  a settled alak marad az alapeset, az eltérést pedig a `glossary.md`-ben indokoljuk, forrással.
- **Ha két angol szöveg csak IGEIDŐBEN tér el, a magyar se hozzon be új szerkezetet.** A
  `errorReporter.dialog.detailsToggle` (`Mi kerül elküldésre`) és a testvére, a `errorReporter.amend.detailsToggle`
  (`Mi került elküldésre`) egymás mellett él ugyanabban a funkcióban; a `kerül + -ásra/-ésre` szerkezet megtartása
  varratmentessé teszi a párt, még ha önmagában szebb lenne is egy `-va/-ve` vagy cselekvő alak. Bizonyíték és a többi
  amend-döntés: `glossary.md` § A már elküldött jelentés kiegészítése.
- **Menübe irányításkor a `-ból/-ből` alak a természetes**: `küldj új jelentést a Súgó menüből`. A macOS ugyanezt önöző
  felszólításként írja (`válassza az Apple menü > Rendszerbeállítások elemet`), a menü NEVE onnan jön, a MONDAT a miénk,
  tehát tegező marad.
- **Ha két funkció ANGOLJA betű szerint azonos, a magyarnak is egynek kell lennie** (`desktop-i18n-term-consistency`),
  és ilyenkor a szállított alak nyer, még ha egy újabb kulcscsalád szebb keretet találna is. Ha a kényszerített alak
  csak a család EGY sorát érintené, az egész családot igazítsd hozzá: az olvasó egy felsorolásban látja őket egyszerre,
  a két funkció eltérését viszont soha. Eset és érvelés: `glossary.md` § A megszakított visszagörgetés
  eredményértesítése.
- **Egy PDF oldala `oldal`, soha nem `lap`**: a `lap` a `tab` foglalt szava. Összetételben kötőjellel: `PDF-oldalak`.
  Fotó esetén a hely `hol készült` / `készítési helye`, a gép adatai `kameraadatok`. Forrás: `glossary.md` § Belenézés a
  fájlokba.
- **Az `askCmdr.tool.*` címkepár akkor is a családi mintát követi, ha a próza más igét használ**: a hozzájárulási szöveg
  `belenéz`-e az eszközsoron `Fájlok átnézése` / `Fájlok átnézve` lesz, mert a `belenéz`-nek nincs állapotot mondó
  `-va/-ve` alakja. Indoklás: `glossary.md` § Belenézés a fájlokba.
- Record case-by-case rulings here so they aren't relitigated.

## Open terms (resolved by evidence, not by David)

David does NOT break ties for Hungarian. He uses shipped Hungarian as his gauge for the whole language-agnostic
pipeline, so hand-feeding it a native gut-check would contaminate that gauge (see `docs/guides/i18n-translation.md` §
Treat every language the same). These resolve the same way they'd resolve for a language no one here speaks: triangulate
the reference pile (including the file-manager sources and the four mining gotchas in § Researching terms), pick the
best-evidenced fit, record residual confidence. No Hungarian-specific input.

- **Address style: `te` (informal), high** — consumer-brand evidence; see Formality and
  `../formal-informal-decisions.md`.
- **pane — settled to `panel`, `high`** (2026-08-19, native-menu pass): Double Commander hu and Total Commander hu both
  say "panel", and the orthodox pair is Cmdr's own UI family. Resolved by evidence, exactly as intended.
- **bookmark, viewer, listing — still tentative.** No Tier-1 source (Finder has no own viewer term), so these need the
  file-manager sources to settle. The next glossary pass mines them like any language; until then they stay open, not
  parked for David.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/hu/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
