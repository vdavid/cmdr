# Hungarian (hu) translation style guide

Working notes for translating Cmdr into Hungarian. Read `../README.md` for how this fits the translation process, and
the app-wide `docs/style-guide.md` for the English voice these notes carry into Hungarian. Term rulings live in
`terms.json` (keyed by the shared `../concepts.json` plus `concepts-proposed.json`), their rationale in `decisions.md`,
and open questions in `review-queue.md`.

macOS DOES ship a Hungarian UI localization (Finder, AppKit, System Settings), so it's the highest-authority source
here, exactly as on other languages. Microsoft Windows Hungarian plus the Microsoft Hungarian style guide are Tier 2,
and the GNOME Nautilus and Xfce Thunar file-manager catalogs are Tier 3 (cross-language parity), with Total Commander
and Double Commander as the two-pane family.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address**: informal `te` (tegezés) in every sentence that speaks to the user (`Megpróbálod újra?`, `Próbáld újra.`);
  never önözés, even where Apple's sentence önöz. Labels don't address anyone.
- **Register by UI slot**:
    - buttons, menu items, window titles, Settings labels, commands, aria labels: nominal (`Másolás`, `Lap bezárása`,
      `Rejtett fájlok megjelenítése`, `X megjelenítése` for Show X, `X keresése…` for a search placeholder);
    - a dialog title that asks: a `te` question with definite conjugation (`Törlöd az AI-modellt?`,
      `Elküldöd a jelentést?`);
    - a status cell: one word or a `-va/-ve` participle (`Fut`, `Várakozik`, `Kész`, `Szüneteltetve`, `Kijelentkezve`);
      running prose says `folyamatban van`;
    - a to-do checklist row is a `te` imperative (`Csillagozd meg a repót a GitHubon`), since one row must be a
      sentence.
    - command descriptions: 3rd-person present (`Megnyitja…`) or a nominal phrase, per family.
- **Voice**: friendly, concise, calm. Never a bare `hiba` or `sikertelen` label: "Couldn't X" → `Nem sikerült X-ni`,
  "Cmdr couldn't X" → `A Cmdr nem tudta X-ni`, "Something went wrong" → `Valami nem sikerült`. The prose word for a
  problem is `probléma`, never `gond`. No apology in a notice that reports a deliberate choice.
- **Localize what Apple localizes**: `Gyorsnézet` (Quick Look), `Infó megjelenítése` (Get Info), `Zárolt` (Locked,
  quoted in prose), `Kulcskarika-elérés` (the app) / `kulcskarika` (the store), `Rendszerbeállítások`,
  `Rendszerintegritás-védelem`, `Lemezkezelő` / `Elsősegély`, `Tevékenységfigyelő`, `Karaktermegtekintő`,
  `Szövegszerkesztő` (TextEdit), `Megtekintő` (Preview), `Alkalmazások` (never `Programok`),
  `Teljes hozzáférés a lemezhez`. Kept English: Finder, Dock, Spotlight, Mission Control, Terminal (the app), Apple
  silicon, Safari. The LABEL is Apple's, the SENTENCE is ours.
- **Brand suffixes**: `Cmdr` takes front-vowel suffixes with no hyphen (`Cmdrt`, `Cmdrben`, `Cmdrnek`, `Cmdrrel`,
  `Cmdrből`), article `a Cmdr`; so do `Dock` / `Finder` (`a Dockban`, `a Finderben`) and `Mac` (`Macen`, `Macet`).
  Acronyms and silent-final names take a hyphen (`AI-t`, `NAS-t`, `adb-t`, `Homebrew-ban`, `USB-kábel`). A name whose
  last letter doesn't spell a Hungarian sound gets a base word instead (`az Android platform tools csomag`,
  `az AlternativeTo oldalán`, `az Escape billentyűvel`).
- **Placeholders never take a suffix or a guessed article.** Dodges, in order of preference: a colon slot
  (`itt: {path}`, `ide: {destination}`, `Letöltve: {fileName}`); a postposition (`{duration} óta`, `{name} szerint`,
  `{secondsText} másodperc múlva`); a base noun that carries the suffix (`a(z) {volumeName} meghajtón`,
  `a(z) „{name}” szervert`, `{host} kulcsát`); the possessor slot (`{name} szerkesztése`, `{name} fotói`). An article in
  front of a name is `A(z) „{name}”` (quotes only around a name the user typed or owns; a brand or app name is bare
  `a(z) {app}`). A value with one possible first sound gets the real article (`az **{name}**` for iCloud Drive). In
  front of a number prefer `Az összes X ({N})` over `Mind a(z) N X`. A duration before `ideig` takes `-nyi`.
- **Plurals**: CLDR `one` / `other`. The noun after a numeral stays SINGULAR in both branches (`3 fájl`, never
  `3 fájlok`), and a numeral subject takes a singular verb; a later clause may go plural.
- **Punctuation**: sentence case; quotes `„…”`; the single character `…` everywhere; `%` tight against the number
  (`42%`); en dash for ranges. Settings paths keep English's separator (`>` or `›`) and follow `itt:`. RAW families
  (`menu.*`, `errors.*`) use single apostrophes, ICU families double them.
- **Numbers**: multipliers spelled out (`négyszer lassabb`, never `4x`); numbers, sizes, and dates come from the
  formatter; `{duration}` is never localized, so it only stands before a postposition.
- **Top traps** (details in `terms.json`):
    - cancel: `Mégsem` (dialog button) vs `Megszakítás` (a running operation) vs `Leállítás` (a service or search).
    - operation → `művelet`; the queue → `Műveleti sor` (never `Műveletsor`); the log → `Műveletnapló`; transfer →
      `átvitel` only for a copy or move in flight.
    - dismiss → `Elvetés` (never `Bezárás`, that's Close); undo → `Visszavonás`; put back → `visszahelyezés` for trashed
      items, `visszaállítás` for an old name; rollback → `visszagörgetés`.
    - pane → `panel`; tab → `lap` (a PDF page is `oldal`); drive → `meghajtó`; volume → `kötet`; device → `eszköz`,
      phone → `telefon`; item → `elem`; server → `szerver` (never `kiszolgáló`); host → `gép` / `gazdagép`.
    - deselect → `kijelölés törlése` (never `megszüntetése`); read-only → `csak olvasható` (never `írásvédett`); scan →
      `átvizsgálás`, a live folder walk → `átnézés`, the Search feature → `keresés`.
    - connect: the pane line `Kapcsolódás ide: {name}…`, Connected `Kapcsolódva`, the Connect button `Csatlakozás`,
      reconnect `újracsatlakozás`, disconnect `leválasztás`, the connection dropping on its own `megszakad a kapcsolat`.
    - Retrying → `Újrapróbálás`, Try again → `Próbáld újra`; `Example:` → `Példa:`, never `Például:`.
    - excluded → `kizárva`, skipped → `kihagyva`; busy menu items → base label + ` (foglalt)`.

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
- **Cancel is "Mégsem"** (the macOS Finder button label), not "Mégse" or "Visszavonás" (that's undo). This is a real
  macOS-vs-Windows split and macOS wins here: `decisions.md` § A macOS-vs-Windows hasadások.

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json` and `concepts-proposed.json`:
`chosen`, accepted forms, usage notes, forms to avoid with the reason, a confidence (`confirmed` / `high` /
`tentative`), and sources. Tier order is macOS (Tier 1) → Microsoft (Tier 2) → the file-manager catalogs (Tier 3); a
vendor's own Hungarian UI (Apple, Android) beats a `@key` description. Where macOS and Microsoft split (Kuka vs Lomtár,
szerver vs kiszolgáló, Mégsem vs Mégse), macOS wins: `decisions.md` § A macOS-vs-Windows hasadások. Rationale worth more
than a line sits in `decisions.md` under a heading that cites its keys, and the term's `decision` field names that
heading. Never guess a term: mine the reference pile first (`../reference-pile/how-to-mine.md`).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Safari, plus the `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`). Apple feature names
Apple localizes are NOT on that list and get translated: Quick Look is `Gyorsnézet` (`terms.json` `quick-look`).

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
  mellett látja. Bizonyítékok és kivételek: `decisions.md` § Natív menük.
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
  the families that were corrected to it: `decisions.md` § A megszakított visszagörgetés eredményértesítése.
    - **Quotes only around a NAME the user typed or owns.** A brand or provider placeholder takes bare `a(z) {name}`
      (`a(z) **{name}** kezeli`): the bold or the sentence already delimits it, and `„Dropbox”` reads as scare quotes.
    - **A placeholder with ONE possible value gets the real article, never the hedge.** `errors.provider.iCloud.*`'s
      `{name}` is always `iCloud Drive`, so it's `az **{name}**`. The `a(z)` form answers an UNKNOWN first sound; where
      nothing is unknown it's just noise.
    - **Read the whole string: one key often has two or three article sites.** The `errors.provider.appBased.*` lines
      carry `a(z) **{name}**`, `a(z) {app} appot`, and `a(z) {name} állapotoldalát`. Fixing the first and moving on
      leaves a half-corrected family, which is worse than either end state.
    - **In front of a NUMBER the article varies too, so ❌ never a bare `a {countText}`.** It follows the numeral's
      pronunciation: `a három`, `a négy`, but `az öt`, `a száz` but `az ezer`. In running prose the hedge is the answer
      and two shipped keys use it (`fileExplorer.imageIndex.folder.allIndexed`, `ui.loadingIcon.finalizing`); don't
      sweep those.
    - **For the phrase "all N X" specifically, prefer `Az összes X ({N})` over `Mind a(z) N X`.** macOS Hungarian words
      it that way (`Az összes lemez (^0) kiadásához…`), and it's strictly better: the article now agrees with `összes`,
      a word we choose, so nothing hinges on the runtime value at all. Worth the swap wherever the count can move to a
      parenthetical or behind a colon, and near-mandatory in a short button, where the hedge is most visible. Worked
      case: `decisions.md` § A megszakított visszagörgetés eredményértesítése.
- **Sentence case is native** (Hungarian doesn't capitalize common nouns, days, or months), so the app's sentence-case
  rule applies cleanly. Don't capitalize the word after a colon unless it's a proper noun.
- **Suffix the brand WITHOUT a hyphen: `Cmdrt`, `Cmdrben`, `Cmdrrel`, `Cmdrnek`, `Cmdrtől`, `Cmdrre`.** `Cmdr` is
  pronounced "commander", so its final written `r` does spell its final pronounced sound, and AkH's hyphen rule (silent
  final letter or an unusual letter cluster spelling the last sound) doesn't apply. Vowel harmony keys off the spoken
  form and follows the word's LAST vowel, which in "commander" is `e`, so the FRONT-vowel suffixes are the right ones
  (`-ben`, `-nek`, `-től`, `-re`, `-hez`, `-ből`), never `-ban`/`-nak`/`-tól`. The shipped catalog is unanimous on this
  (24× `Cmdrnek`, 9× `Cmdrben`, 4× `Cmdrből`, 3× `Cmdrhez`, 2× `Cmdrre`). Same for the multiword product name:
  `Ask Cmdrt`.
- **A foreign name whose final letter doesn't spell a Hungarian sound gets a BASE WORD, not a suffix.** `AlternativeTo`
  is `az AlternativeTo oldalán`, because the English `o` is neither a hyphen case (`AlternativeTo-n`) nor a lengthening
  case (`AlternativeTón`) you could defend. Same move as `az Android platform tools csomag` (`terms.json`
  `android-platform-tools`): the base word leaves the name uninflected and the sentence stays short.
- **The nominal-label rule stops at a checklist of things to DO.** `onboarding.stepBeta.checklist.*` rows are link texts
  in a to-do list, and one of them (`…checklist.email`) has to be a sentence wrapped around an inline input, so the
  whole row set is informal imperative (`Csillagozd meg a repót a GitHubon`), not nominal. A mixed form is visible
  inside one list; a difference between two lists never is.
- **Neither GitHub nor AlternativeTo ships a Hungarian UI**, so "use the site's own verb" has no Tier-1 answer for
  `star` or `like` — a Hungarian user sees the English buttons. Microsoft terminology decides both (`csillagoz`,
  `kedvel`); evidence in `decisions.md` § A bevezető átírt lépései.
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
  `gond` (arra a `hu` pile nulla találatot ad). Forrás és teljes érvelés: `decisions.md` § Ha a Cmdr nem állt le.
- **A macOS panelneveit magyarul írjuk, mert az Apple is lefordítja őket.** `Get Info` → `Infó megjelenítése`, `Locked`
  → `Zárolt`, `Sharing & Permissions` → `Megosztás és jogok`. Egyik sincs a `BRAND_WORDS` listán, tehát az
    1. terminológiai alapelv (fordítsd, amit az Apple fordít) érvényes rájuk. A CÍMKÉK az Apple-éi, a MONDAT a miénk:
       tegezünk és köznyelvi maradunk (`vedd ki a „Zárolt” pipát`), nem másoljuk az Apple önöző hivatalnyelvét
       (`szüntesse meg a … kijelöltségét`). Bizonyítékok: `decisions.md` § A macOS-panelnevek magyarul.
- **Ugyanaz az angol mondat KÉT különböző magyar alakot kaphat, ha a burkoló szöveg eltér.** A `errors.eject.unexpected`
  és a `errors.mutation.unexpected` angolul betű szerint azonos, magyarul mégsem az: az előbbi a
  `Nem sikerült kiadni: …` burkoló után áll, ahol a settled `Valami nem sikerült` közvetlen szóismétlés lenne. Ilyenkor
  a settled alak marad az alapeset, az eltérést pedig a `decisions.md`-ben indokoljuk, forrással.
- **Ha két angol szöveg csak IGEIDŐBEN tér el, a magyar se hozzon be új szerkezetet.** A
  `errorReporter.dialog.detailsToggle` (`Mi kerül elküldésre`) és a testvére, a `errorReporter.amend.detailsToggle`
  (`Mi került elküldésre`) egymás mellett él ugyanabban a funkcióban; a `kerül + -ásra/-ésre` szerkezet megtartása
  varratmentessé teszi a párt, még ha önmagában szebb lenne is egy `-va/-ve` vagy cselekvő alak. Bizonyíték és a többi
  amend-döntés: `decisions.md` § A már elküldött jelentés kiegészítése.
- **Menübe irányításkor a `-ból/-ből` alak a természetes**: `küldj új jelentést a Súgó menüből`. A macOS ugyanezt önöző
  felszólításként írja (`válassza az Apple menü > Rendszerbeállítások elemet`), a menü NEVE onnan jön, a MONDAT a miénk,
  tehát tegező marad.
- **Ha két funkció ANGOLJA betű szerint azonos, a magyarnak is egynek kell lennie** (`desktop-i18n-term-consistency`),
  és ilyenkor a szállított alak nyer, még ha egy újabb kulcscsalád szebb keretet találna is. Ha a kényszerített alak
  csak a család EGY sorát érintené, az egész családot igazítsd hozzá: az olvasó egy felsorolásban látja őket egyszerre,
  a két funkció eltérését viszont soha. Eset és érvelés: `decisions.md` § A megszakított visszagörgetés
  eredményértesítése.
- **Egy PDF oldala `oldal`, soha nem `lap`**: a `lap` a `tab` foglalt szava. Összetételben kötőjellel: `PDF-oldalak`.
  Fotó esetén a hely `hol készült` / `készítési helye`, a gép adatai `kameraadatok`. Forrás: `decisions.md` § Belenézés
  a fájlokba.
- **Az `askCmdr.tool.*` címkepár akkor is a családi mintát követi, ha a próza más igét használ**: a hozzájárulási szöveg
  `belenéz`-e az eszközsoron `Fájlok átnézése` / `Fájlok átnézve` lesz, mert a `belenéz`-nek nincs állapotot mondó
  `-va/-ve` alakja. Indoklás: `decisions.md` § Belenézés a fájlokba.
- **A `Mac` helyhatározós (superessivusi) alakja a katalógusban `Macen`, kötőjel nélkül** (`settings.mediaIndex.*` három
  helyen), a birtokos alak viszont `Mac-eden` (`settings.updates.emailPrivacyNote`). Új szövegben a többségi `Macen`
  alakot használd; a kettősség ismert, de egy fordítási menet ne söpörje át a többi kulcsot.
- **Az „adb”-féle parancsnevek kisbetűsek maradnak, és a ragjuk kötőjeles** (`az adb-t`, mert betűszóként á-dé-bé a
  kiejtése, tehát a névelő is `az`). Ugyanígy `ADB-n át`, `Android SDK-ban`, `Homebrew-ban` (a `w` néma, ezért kötőjel).
  Prózában idézőjelbe kerül, ahogy az angol is idézi: `az „adb” parancs`.
- **A kiszürkített („busy”) menüpont az alapcímke + ` (foglalt)`**: az alapváltozat szövege betűre változatlan marad, és
  csak a záró ` (foglalt)` kerül a végére (`menu.volume.ejectBusy`, `menu.volume.disconnectBusy`,
  `menu.volume.forgetServerBusy`, `menu.volume.forgetSavedPasswordBusy`). A `(foglalt)` névszói állapotjelző, ezért
  bármelyik címke után áll, akár ige, akár főnévi szerkezet az alap. Egyetlen jelölő van; új „busy” kulcs ne találjon ki
  másikat. Forrás: `decisions.md` § busy.
- **Az oszlopcímeknél a betű szerinti Apple-találat veri a katalógus családi mintáját.** A `Last used` azért
  `Utolsó használat` (macOS `Security.prefPane`, ugyanez a szerep: táblázat-oszlopcím) és nem `Utoljára használva`,
  pedig a fájllista dátumoszlopai `-va/-ve` alakúak (`Módosítva`, `Létrehozva`): azoknál nincs Tier-1 forrás a konkrét
  szóra, itt van. Bizonyíték: `decisions.md` § A szerverközpont táblázata.
- **A `Mac` tárgyesete `Macet`, kötőjel nélkül** (macOS-attesztált, 161 találat), a `Macen` alakkal egy tőről.
- **A `reconnect` töve a katalógusé (`újracsatlakoz-`), a panelcím KERETE viszont a családé (`ide: {name}…`).** A
  `servers.paneState.reconnecting` ezért `Újracsatlakozás ide: {name}…`: a tő a szállított
  `errors.listing.deviceReconnecting.title` (`Újracsatlakozás az eszközhöz`) és a vele egy nézetben látszó két
  testvérkulcs alakja, a keret a `servers.paneState.connecting` (`Kapcsolódás ide: {name}…`) idiómája. A macOS a másik
  tőre is ad Tier-1 találatot (`Újrakapcsolódás…`), de a szállított alak nyer, és egy nézeten belüli tőváltás rosszabb,
  mint két nézet közötti. Bizonyítékok: `decisions.md` § Az automatikus újracsatlakozás.
- **Az `Ask Cmdr` MÁRKANÉV csak a csevegőpanelt nevezi meg, a próza `a Cmdr`-ről vagy `az AI`-ról beszél.** Az angol
  ugyanezt a vonalat húzza: `Ask Cmdr` maradt a panel címében, a Nézet menüben, a parancspalettán, a beállítási
  szakaszban és a be-/kikapcsolóban, mindenhol máshol `Cmdr` vagy `the AI` áll. A magyarban ez azt jelenti, hogy egy
  mondat alanya `a Cmdr` (`A Cmdr figyeli azokat a mappákat…`), egy felületnév viszont marad `az Ask Cmdr beállításai`.
  ❌ Ne írd vissza az `Az Ask Cmdr…` alakot a mondatokba: ott már nem a panelről van szó, hanem arról, amit az app
  csinál.
- **Ahol az angol `the AI`-t mond, a magyar `az AI`, nem `a Cmdr`.** A `suggestedOps.*` család (`Az AI indoka`,
  `Ezeket az AI javasolta`) szándékosan a modellt nevezi meg, nem az appot: a szomszédos `suggestedOps.cmdrFacts`
  (`Amit a Cmdr tud`) épp az ellentétét állítja (amit az app ELLENŐRZÖTT), és a kettőnek különböző alanyt kell kapnia,
  különben elveszik a kulcs egész értelme.
- **A `könyvtár` a katalógusban a `directory` szava, ezért egy „library” sosem lehet puszta `könyvtár`.** Az Ollama
  modellgyűjteményére ezért `modellkönyvtár` áll (`onboarding.cloudSetup.step.ollamaModel`): a `modell-` előtag oldja
  fel az ütközést, és egy fájlkezelőben a puszta „böngészd a könyvtárat” tényleg mappaböngészésnek olvasódik. Ugyanez a
  minta, mint az `Android-fejlesztőeszköz`-nél: az előtag menti meg a foglalt szót.
- **Az `LM Studio` ragozva `LM Studióban`, kötőjel nélkül**, mert a magyar a végső `-o`-t megnyújtja a toldalék előtt
  (`Chicagóban`, `Oslóban`). Nem szerepel a `BRAND_WORDS` listán, tehát a don't-translate ellenőrzés nem is nézi, de a
  márkanév felismerhető marad.
- **`csevegés` = a csevegés mint tevékenység és a panel; `beszélgetés` = egy szál.** Az angol `chat` és `conversation`
  nem áll szigorúan szemben egymással, a magyar viszont a szállított katalógusban végig így osztja:
  `Nyisd meg a csevegést` (panel), `Új csevegés`, de `Ezt a beszélgetést a Cmdr kezdte` és
  `A Cmdr kezdeményezhet beszélgetést` (egy konkrét szál). Új kulcs ezt kövesse.
- **A `Dock` és a `Finder` angolul marad, és a ragjuk kötőjel nélkül tapad**: `a Dockban`, `a Dockodban`, `a Dockomba`,
  `a Finderben`, `a Finder mellé`. Mindkettő végi betű a kiejtett hangot írja, tehát az AkH kötőjelszabálya nem lép be.
  Összetételben viszont kötőjel jár (`Finder-ablak`, `Dock-ajánlat`). Az `Applications` mappa magyar neve `Alkalmazások`
  (az Apple lefordítja). Bizonyítékok: `decisions.md` § A Dockba kerülés egyszeri ajánlata.
- **A Dock helyi menüjének szövegeit magából a `Dock.app`-ból mérd, ne a kupacból.** A referenciakupac `hu/macOS/`
  mappája csak a Findert, az AppKitet és a System Settingset tartalmazza; a Dock saját menüje a
  `/System/Library/CoreServices/Dock.app/Contents/Resources/hu.lproj/DockMenus.strings` fájlban él
  (`plutil -convert json`), és pontosan az a felület, amelybe a `menu.dock.*` elemek kerülnek. Onnan jön az appnevek
  mintája: **puszta név + névszói cselekvés, névelő nélkül** (`%@ elrejtése`, `Cmdr megnyitása`); az
  `A(z) „%@” megnyitása` hedge csak FÁJLNÉVRE való, ahol a kezdőhang ismeretlen. Bizonyítékok: `decisions.md` § A Dock
  helyi menüje.
- **⚠️ ❌ A `Programok mappa` soha nem jön vissza**: az a régi Mac OS X-es név, nem a mai macOS-é. Semmilyen ellenőrzés
  nem fogja el a visszaesést, mert az érintett kulcsok angolja nem betű szerint azonos.
- Record case-by-case rulings here so they aren't relitigated.

## Open terms (resolved by evidence, not by David)

David does NOT break ties for Hungarian. He uses shipped Hungarian as his gauge for the whole language-agnostic
pipeline, so hand-feeding it a native gut-check would contaminate that gauge (see `docs/guides/i18n-translation.md` §
Treat every language the same). Open terms resolve the same way they'd resolve for a language no one here speaks:
triangulate the reference pile (including the file-manager sources and the mining gotchas in
`../reference-pile/how-to-mine.md`), pick the best-evidenced fit, record residual confidence. Address style is settled
(`te`, high; `../formal-informal-decisions.md`) and pane is settled (`panel`, from Double Commander and Total
Commander). The rest wait in `review-queue.md`.

## Termbase files

- `../concepts.json`: the shared, language-agnostic concept registry (sense, `match` patterns, confusable neighbors);
  `concepts-proposed.json` holds this locale's proposals until they merge into it.
- `terms.json`: this locale's ruling per concept, with the catalog keys that legitimately deviate under `exceptions`.
- `decisions.md`: the rationale journal, one section per feature, headings citing their keys.
- `review-queue.md`: open questions for a native reviewer.

Add or change a ruling in place in `terms.json` (a replaced form moves to `avoid`), and add a `decisions.md` section
when the reason needs more than a line. Source every term from the reference pile (`_ignored/i18n/hu/`; recipes in
`../reference-pile/how-to-mine.md`).
