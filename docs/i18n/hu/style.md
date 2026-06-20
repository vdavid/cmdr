# Hungarian (hu) translation style guide

Working notes for translating Cmdr into Hungarian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Hungarian.

macOS DOES ship a Hungarian UI localization (Finder, AppKit, System Settings), so it's the highest-authority source
here, exactly as on other languages. Microsoft Windows Hungarian plus the Microsoft Hungarian style guide are Tier 2,
and the GNOME Nautilus and Xfce Thunar file-manager catalogs are Tier 3 (cross-language parity). Glossary entries below
cite which sources back each choice.

## Voice and tone

Friendly, concise, active, calm. Hungarian software leans on a nominal style for labels (a button is a noun, "Másolás" =
"Copying", not a command), which reads clean and native rather than cold. Conversational copy stays warm and uses the
informal `te` address (see Formality). Error messages stay calm and actionable and
never use "hiba" (error) or "sikertelen" (failed) as a bare label: state the problem and a next step ("Nem sikerült
átnevezni a fájlt. Megpróbálod újra?").

## Formality

**Verdict: informal `te` (tegezés) throughout. No önözés.** Consumer brands (IKEA, Spotify, Netflix, H&M, Coca-Cola)
all address Hungarian users with `te`, which fits Cmdr's friendly personal voice. The OS sources lean önözés, but
Cmdr deliberately picks the warmer consumer-brand register. Formality decision recorded in
[`formal-informal-decisions.md`](../formal-informal-decisions.md).

- **Labels (buttons, menus, headers): nominal / infinitive, no direct address.** "Másolás", "Áthelyezés", "Törlés",
  "Mégsem". The dominant Hungarian UI convention; macOS Finder, Microsoft, GNOME, and Xfce all do this, and it sits
  fine under a `te` register since a label isn't direct address.
- **Conversational copy and questions: `te` (tegezés).** Where English addresses the user directly, use the informal
  second person. A friendly question is "Megpróbálod újra?", not the önözés "Megpróbálja újra?".
- **Cancel is "Mégsem"** (the macOS Finder button label), not "Mégse" or "Visszavonás" (that's undo). See the glossary
  note: this is a real macOS-vs-Windows split and macOS wins here.

## Terminology and glossary

Format: each line is `English: chosen · sources · confidence`. Confidence is `confirmed` (a human signed off), `high`
(authoritative sources agree), or `tentative` (sources conflict or none had it). Sources: mac = macOS Finder/AppKit,
ms = Microsoft terminology/style guide, gn = GNOME Nautilus, xf = Xfce Thunar. Contested terms get a short block.

- pane: `panel` · no Tier-1 source (macOS Finder is single-pane; macOS "panel" means a Settings pane) · tentative. The
  two file lists. Microsoft's literal term is "ablaktábla"; "panel" is cleaner and idiomatic for a UI region. Flagged.
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
- disconnect (network): `leválasztás` · mac ("Leválaszt", "Kapcsolat bontása") · high.
- share (an SMB share): `megosztás` · mac, ms, gn · high.
- search: `keresés` · mac ("Keresés"), ms, gn, xf · high.
- sort: `rendezés` · mac ("Rendezés módja"), ms · high.
- settings: `beállítások` · mac ("Beállítások"), ms · high.
- download: `letöltés` · mac, ms · high.
- index / indexing: `index` / `indexelés` · ms ("index") · high.
- overwrite: `felülírás` · mac ("Felülír"), ms, xf ("Felülírja?") · high.

Contested or split, with the per-source evidence:

### trash → `Kuka`

- mac: `Kuka` (30 occurrences), zero `Lomtár`.
- ms: gives both `kuka` and `lomtár`, but reserves `Lomtár` specifically for the Windows "Recycle Bin" product name.
- gn: `Kuka` ("Kukába dobva", "_Kuka ürítése").
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
- Chosen: `könyvjelző` · source gn, plus mac's literal usage · tentative. macOS/MS `Kedvenc` names a Favorites *sidebar
  concept*, not an explicit bookmark action; for Cmdr's named bookmark feature the file-manager-native `könyvjelző`
  (GNOME) is clearer. Flagged for David: pick `könyvjelző` (literal, GNOME) vs `kedvenc` (macOS sidebar feel).

### cancel → `Mégsem`

- mac: `Mégsem` (52 occurrences, the actual button label), zero `Mégse`.
- ms: `Mégse` (terminology and style-guide examples).
- gn/xf: `Mégse` ("_Mégse", "Mé_gse").
- Chosen: `Mégsem` · source mac (Tier 1) · high. A genuine macOS-vs-Windows/Linux split. The earlier draft asserted
  `Mégse` and explicitly rejected `Mégsem`; macOS Finder, the highest authority and what the user sees, uses `Mégsem`,
  so Cmdr follows macOS. Never "Visszavonás" (undo).

Add lines as terms come up, keeping the `chosen · sources · confidence` format.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('hu')`; matches the GNOME/Xfce catalogs'
`nplurals=2; plural=(n != 1)`). You must still write both branches because ICU requires them, but mind the grammar:

- **Hungarian does NOT pluralize a noun after a number.** "1 fájl" AND "3 fájl", never "3 fájlok". The counted noun is
  singular in both the `one` and `other` branches; the branches differ only in any other agreement, not in the noun
  ending. Confirmed in the GNOME Nautilus catalog, where a counted-files entry keeps the singular noun in both plural
  slots (`msgstr[0]` = "%'d mappa kijelölve" AND `msgstr[1]` = "%'d mappa kijelölve", never "mappák"). This is the
  single biggest plural gotcha for Hungarian.
- No grammatical gender, which removes a whole class of agreement problems.

## Notes and decisions

- **Agglutination + vowel harmony makes suffixed placeholders dangerous.** Hungarian attaches case suffixes that must
  harmonize with the word's vowels (`-ban`/`-ben`, `-ról`/`-ről`, `-hoz`/`-hez`/`-höz`) and sometimes double a final
  consonant. A `{path}` or `{name}` whose value is unknown can't take a correct suffix ("{path}-ban" may be wrong).
  Restructure so a placeholder isn't suffixed: put it after a postposition or in a neutral slot ("itt: {path}", not
  "{path}-ban").
- **Definite vs indefinite conjugation and the `a`/`az` article** depend on the following word, so phrasing around a
  placeholder needs care; prefer constructions that don't hinge on the inserted value's first sound.
- **Sentence case is native** (Hungarian doesn't capitalize common nouns, days, or months), so the app's sentence-case
  rule applies cleanly. Don't capitalize the word after a colon unless it's a proper noun.
- **Quotation marks: `„…”`** (low opening, high closing) is the standard Hungarian form. macOS Finder uses it too
  (e.g. „^0”). Avoid English `"…"`.
- **Numbers and dates come from the formatter layer.** Hungarian uses a comma decimal and space thousands separator, and
  a native `YYYY. MM. DD.` date order; `formatNumber()`/`formatBytes()`/the date formatters produce these from the
  locale. Never hardcode separators or date order in a string.
- **Length** runs near English; still overflow-check against the pseudolocale (`en-XA`).
- Record case-by-case rulings here so they aren't relitigated.

## Decisions to confirm with David

David is the native expert here. Everything above is grounded in the sources; these are the ones still worth a native
gut-check.

- **Address style: RESOLVED to informal `te`** (consumer-brand evidence; see Formality and
  [`formal-informal-decisions.md`](../formal-informal-decisions.md)). No longer open.
- **bookmark → `könyvjelző` vs `kedvenc`** (tentative). `könyvjelző` is the literal, file-manager-native choice (GNOME);
  `kedvenc` is what macOS calls its Favorites sidebar. Pick one.
- **pane → `panel`** (tentative). No Tier-1 source (Finder is single-pane). `panel` reads clean; `ablaktábla` is the
  Microsoft literal. Confirm `panel`.
- **viewer → `megjelenítő`** and **listing → `fájllista`** (tentative). Both are reasonable descriptive coinages with no
  exact Tier-1 match. Confirm or adjust.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/hu/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
