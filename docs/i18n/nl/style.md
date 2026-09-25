# Dutch (nl) translation style guide

Working notes for translating Cmdr into Dutch. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Dutch. Term rulings live in `terms.json`
(keyed by the shared `../concepts.json`), their rationale in `decisions.md`, and open questions in `review-queue.md`.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address**: informal `je` / `jij` / `jou` / `jouw` everywhere, like macOS Dutch (zero `u` in Finder or AppKit).
  Unstressed `je` by default; `jij` / `jouw` only for contrast ("tot jij het goedkeurt"). Don't force `je` into lines
  macOS phrases neutrally ("Versturen…").
- **Voice**: friendly, concise, active, calm. Error copy states the problem and a next step and never uses `fout` or
  `mislukt` as a label: "X failed" → `X lukte niet`, "Couldn't X" → `Kon … niet` / `X lukte niet`, a stopped operation →
  `Niet voltooid`, "Something went wrong" → `Er ging iets mis`. Compounds like `foutrapport` and `typefout` are fine. No
  apologies in notices that report a deliberate choice.
- **Register by UI slot**:
  - buttons and menu items: bare-stem imperative, like Finder (`Verstuur`, `Annuleer`, `Kopieer`, `Toon`, `Stop`), even
    when only a screen reader hears the label;
  - window and dialog titles, yes/no questions: infinitive (`Foutrapport versturen`, `Deze bewerking terugdraaien?`);
  - Settings labels (checkbox, field): infinitive last (`Verborgen bestanden tonen`); their descriptions: imperative;
  - progress: `Bezig met …` for a running operation or queue row, bare infinitive plus ellipsis for a short inline line
    (`Zoeken…`, `Verbinden met {name}…`); a heading uses the infinitive, a full sentence the finite verb.
  - A separable verb keeps its particle at the END: `Voeg aan favorieten toe`, `Werp {name} uit`, `Pas … toe` (Apple's
    verbatim `Voeg toe aan Dock` is the one exception).
- **Menu-bar and Apple names follow the Dutch macOS**, even when odd: `Archief` (File), `Wijzig` (Edit), `Weergave`
  (View), `Ga`, `Venster`, `Voorzieningen`, `Vergroot/verklein` (Window > Zoom), `Toon info` (Get Info), `Beveiligd`
  (Locked), `snelle weergave` / `Geef snel weer` (Quick Look), `Sleutelhanger`, `Teksteditor`, `Voorvertoning`, the
  folder `Apps`. Localize what Apple localizes, whatever a `@key` description says. Kept English: Finder, Spotlight,
  Terminal, Disk Utility, First Aid, Activity Monitor, Mission Control, Dock (with `het` in a sentence), Apple silicon,
  System Integrity Protection. On a phone, Android's own Dutch wins (`USB-foutopsporing`, `Toestaan`, `tik op`).
- **Capitalization**: sentence case; only the first word and proper nouns.
- **Typography** (`mechanics.json`): quotes `‘…’`, nested `“…”`, as macOS Dutch quotes UI names; never straight `"` or
  `'…'`. Apostrophe straight or curly (`foto's`, `'s avonds`). `…` for an ellipsis, no space before `%`. Menu-path
  separators (`>`, `→`, `›`) mirror EN per key. ICU values double a straight apostrophe (`foto''s`); RAW families
  (`errors.*`, `menu.*`) don't.
- **No hedged grammar**: never `bestand(en)`, `bestand/en`, `de/het`, `hij/het`, `is/zijn`, or `{name}'s`. Use ICU
  `plural` when Cmdr knows the count; otherwise name the type first (`de map {name}`), use a colon form, or a pronominal
  adverb (below). No `de`/`het` directly before an insert that is the whole noun.
- **Compounds** concatenate (`crashrapport`, `bestandenlijst`); hyphenate before an acronym or English proper name
  (`SMB-share`, `macOS-versie`, `Klembord-PDF`, `het Help-menu`, `Ask Cmdr-model`); format tokens stay lowercase
  (`zip-archief`). A hyphenated first part closes up: `alleen-lezenvolume`.
- **Brand**: `Cmdr`, `macOS`, `GitHub`, `SMB`, `MTP`, `Safari` stay verbatim and take NO genitive-s: `de AI van Cmdr`,
  never `Cmdrs AI` (the don't-translate check reads `Cmdrs` as a dropped brand). `Ask Cmdr` names only the chat panel;
  prose about what the AI does says `Cmdr` or `de AI`.
- **Plurals**: CLDR `one` / `other`. Dutch inflects the verb with the count, so pull only the noun plus its verb into
  the branches and share the rest (`{count, plural, one {# bestand is} other {# bestanden zijn}} nog open`). No definite
  article before a numeral: `Alles is teruggezet: {countText} onderdelen.`
- **Placeholders**: never refer back to an uncontrolled `{name}`, `{path}`, `{app}`, or `{host}` with a pronoun (gender
  unknown); use a pronominal adverb (`daar staat nu iets in`, `er … mee`) or repeat the noun (`Open de server opnieuw`).
  A process name `{app}` leads without an article.
- **Top traps** (details in `terms.json`):
  - operation → `bewerking` (the queue `Bewerkingenwachtrij`, the log `Bewerkingenlogboek`); transfer → `overdracht`
    only for a copy or move in flight.
  - rename → `naam wijzigen` / `Wijzig naam`, noun `naamwijziging`; never `hernoemen`.
  - eject → `Werp … uit` / `uitwerpen`, never macOS's `Verwijder` (that is delete).
  - dismiss → `Sluit`, never `Wis`; clear → `wissen`.
  - quit → `Stop` / `stoppen`, never `Afsluiten`; save → `Bewaar` / `bewaren` (the adjective stays `opgeslagen`).
  - send → `Verstuur` / `versturen`, never `Stuur` or `verzenden`.
  - device → `apparaat`, never `toestel`; drive → `schijf` (a de-word: `hij`/`hem`); item → `onderdeel`.
  - locked file → `beveiligd`, never `vergrendeld`; Brief / Full view → `Beknopte weergave` / `Volledige weergave`,
    never `Kort` / `Volledig`.
  - Edit: `Bewerk` opens an editor, `Wijzig …` opens a form, `Wijzig` is the menu bar.
  - Undo: `Herstel` (Edit menu), `Zet terug` (trash toast), `Ongedaan maken` (Ask Cmdr rename run).
  - A feature switched off, named from elsewhere: `… staat uit`; turning it on in a hint: `Zet … aan`; the Settings
    label: `inschakelen` / `ingeschakeld`. Drive indexing in prose: `het indexeren van schijven`.

## Formality: `je`, settled

**Address the user as `je` / `jij` / `jou` / `jouw`** (informal second person) throughout. This is settled from the
sources, not a guess:

- macOS Dutch is fully informal. Across the mined Finder + AppKit strings, every second-person address uses `je` / `jij`
  / `jou` (520 `je` + 12 `jou` in Finder, 189 `je` in AppKit); there is not a single formal `u` / `uw` address (the one
  `uw` hit in AppKit is not a user address). Finder phrases prompts as "Weet je zeker dat je …", "Je kunt …", "Wil je
  opnieuw proberen …" (verified in `nl/macOS/`, grep over Finder + AppKit, 2026-06-19).
- GNOME Nautilus and Xfce Thunar Dutch use formal `u` ("Wanneer u een bestand via e-mail verstuurt …"). That is the
  open-source desktop convention (Tier 3), not ours; don't copy it.
- Cmdr is a macOS app with a friendly voice that even signs onboarding as David, so `je` is both the macOS-native choice
  and the right tonal fit. Microsoft's `nl` style guide (`microsoft-style-guides/StyleGuide.pdf` in the pile) agrees: it
  records that the historically formal second person has given way to `je` in consumer-facing products, and that the
  Microsoft voice avoids an unnecessarily formal tone (pp. 12, 20, verified 2026-08-21). macOS Tier 1 would decide it
  alone anyway.)

## Voice and tone

Friendly, concise, active, calm, warm. Dutch UI copy reads naturally with direct `je` address; keep it light and don't
force `je` into every line where macOS phrases neutrally ("Versturen…", not "Je verstuurt nu…").

Error messages stay calm and actionable and never use "fout" or "mislukt" as a bare label: state the problem and a next
step. Note: macOS itself does use "fout" freely ("een onverwachte fout"); Cmdr's voice rule is stricter than macOS here,
so don't copy that pattern.

## Formality mechanics

- **`je` / `jij`**, throughout (see Formality above). Use `je` (unstressed) by default; `jij` only for contrastive
  emphasis.
- **Buttons and menu items: bare-stem imperative**, matching macOS Finder, which uses the imperative stem, NOT the
  infinitive: "Verstuur" (not "Versturen"), "Annuleer" (not "Annuleren"), "Kopieer" (not "Kopiëren"), "Toon" (not
  "Tonen"). (verified in `nl/macOS/`, key cross-ref by value, 2026-06-19.)
  - Caveat: the GNOME/Xfce catalogs use the infinitive for buttons ("Verzenden"). macOS is Tier 1 and the imperative
    stem is the native-Mac feel, so prefer it for Cmdr's buttons.
  - When the English button is a verb Dutch has no verb for (English "Background" as an act; there is no
    `achtergronden`), don't fall back to the bare noun, which reads as a label: use the prepositional or directional
    phrase the catalog already uses for the concept ("Op de achtergrond", like the settled "Naar prullenmand"). See
    `decisions.md` § De knop voor een lege wachtrij.
  - **A separable verb keeps its particle at the END of the label**, however far that pushes it: "Add to report" →
    `Voeg aan rapport toe`, not the English-ordered "Voeg toe aan rapport". macOS does exactly this ("Voeg aan begin van
    knoppenbalk toe", "Voeg het lettertype aan de stijl toe", verified in `nl/macOS/AppKit`, 2026-08-28). Same for
    `zet … terug`, `werp … uit`, `koppel … los`.

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json`: `chosen`, accepted forms, usage
notes, forms to avoid with the reason, a confidence (`confirmed` / `high` / `tentative`), and sources. Tier order is
macOS (Tier 1) → Microsoft (Tier 2) → the file-manager catalogs (Tier 3); a vendor's own Dutch UI (Apple, Android) beats
a `@key` description. Rationale worth more than a line sits in `decisions.md` under a heading that cites its keys, and
the term's `decision` field names that heading. Never guess a term: mine the reference pile first
(`../reference-pile/how-to-mine.md`).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style tokens
and any `{email}`-style placeholders. Enforced by `desktop-i18n-dont-translate` (list in
`apps/desktop/scripts/i18n-catalog-lib.ts`). macOS UI names Cmdr opens into (System Settings panes, "Prullenmand")
should match a Dutch macOS.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('nl')`). Write both branches: "1 bestand" /
"{count} bestanden". Dutch is close to English here (singular vs everything-else), so plural handling is low-risk.

The one real trap: Dutch inflects the VERB with the count ("1 bestand **is**" / "3 bestanden **zijn**"), so an English
sentence whose verb sits outside the plural needs the verb pulled INTO the branches. Keep the branches as small as
possible: put only the noun plus its copula inside, and let the rest of the sentence share that one copula
(`{count, plural, one {# bestand is} other {# bestanden zijn}} nog open en misschien al gedeeltelijk geschreven.`).
Duplicating the whole sentence per branch works too but rots twice as fast.

The trap has a second face: a preformatted `*Text` count with NO integer partner gives you no branch to put `is`/`zijn`
in. The fallback is to drop the finite verb and keep the clause elliptical, parallel to a participial neighbour
(`… teruggezet; {skippedText} nog in de prullenmand.`), which reads correctly at every count. Ask for an integer partner
instead where you can: `fileOperations.trash.undonePartial` gained a `{skipped}` driver for exactly this reason, so its
second half is now a normal plural with a real verb
(`… teruggezet; {skippedText} {skipped, plural, one {onderdeel bleef} other {onderdelen bleven}} in de prullenmand.`).

## Notes and decisions

- **Native menu's volgen de Finder-formulering, niet die van de catalogus.** Waar macOS een equivalent heeft, wint dat
  (`Archief`, `Voorzieningen`, `Geef snel weer`, `Vergroot/verklein`), omdat de gebruiker Cmdrs menubalk naast die van
  de Finder ziet. De ene uitzondering is `eject`, waar Apples `Verwijder` met _delete_ zou botsen. Bewijs en
  uitzonderingen: `decisions.md` § Native menu's.
- **The grayed-out menu item keeps its base label and adds ` (bezet)`.** A `*Busy` key is the same command in a second
  state, so it repeats its base item byte-identically and appends one marker, exactly as `menu.volume.ejectBusy`
  (`Werp uit ({name}) (bezet)`) does: `Verbreek (bezet)`, `Vergeet server (bezet)`,
  `Vergeet opgeslagen wachtwoord (bezet)`. `bezet` is the short adjectival form a parenthetical needs and it fits every
  base label unchanged; the `in-use` term's `in gebruik` is for full sentences about a volume, so don't swap one for the
  other. Never invent a second marker: if a future base label can't take `(bezet)`, report it.
- **Sentence case, not title case.** Dutch capitalizes only the first word and proper nouns, which fits the app's
  sentence-case rule directly. "Verstuur crashrapport?" not "Verstuur Crashrapport?".
- **Quotation marks:** macOS Dutch quotes UI names in single quotes (385 single-quoted values in Finder + AppKit
  `nl.lproj`, zero double; verified on macOS 26.6.2, `plutil` over `*.strings`, 2026-09-25), so Cmdr writes `‘…’`, with
  `“…”` nested. `mechanics.json` declares the set and `i18n-mechanics` checks it.
- **Length:** Dutch runs slightly longer than English (compounds like "crashrapport", "instellingen"), but far less than
  German. Overflow-check the layout against the pseudolocale (`en-XA`); watch buttons and toasts.
- **Compound nouns concatenate** ("crashrapport", "foutcode"). Correct Dutch; don't space-separate them.
- **macOS UI names Apple localizes get the Dutch name, even when the English `@key.description` says otherwise.**
  `Get Info` → `Toon info` and the Info-panel `Locked` → `Beveiligd` are both localized by Apple, so term-choice
  principle 1 wins over a source description that predates the rule. Only names Apple itself keeps English (Finder,
  Spotlight, Terminal, Disk Utility, First Aid, Activity Monitor) stay verbatim. Report the clash upward rather than
  silently following the description.
- **An `errors.eject.*` value is read AFTER its wrapper's colon** (`{volumeName} uitwerpen lukte niet: …` /
  `Verbinding verbreken lukte niet: …`), so check the sentence against the wrapper and don't restate the wrapper's verb
  unless English does too.
- **Numbers and dates come from the formatter layer** (comma decimal, period thousands). Never hardcode separators.
- **Speed multipliers**: write them as digits plus `x` and the equality shape, `4x zo langzaam als …` /
  `4x zo snel als …`, not `4x langzamer dan`, which leaves open whether the factor applies to the difference or the
  whole. The Microsoft style guide prescribes digits for units and percentages; the shape itself has no pile precedent,
  so it's a judgment call (see `decisions.md` § De terugvalmelding voor de systeem-SMB-verbinding).
- **An uncontrolled `{name}` NEVER takes a pronoun.** The name can be a file (`het`) or a folder (`de`), so any pronoun
  is wrong half the time. Reach for the pronominal adverb, which works for both genders AND both numbers:
  `daar is iets aan gewijzigd`, `daar staat nu iets in`, `nadat Cmdr er klaar mee was`. Bonus: with no pronoun left, a
  `.named` and a `.counted` sibling can share one sentence, and the counted one keeps a single `plural` block instead of
  three. Worked example: `decisions.md` § De terugdraaimelding.
- **Totality before a count**: completeness first, the number in an apposition after a colon:
  `Alles is teruggezet: {countText} onderdelen.`
- **The volume switcher is `de volumekiezer`** in every string; the old coinage `wisselaar` stays unused.
- **`Wijzig …` opens a form; `Bewerk …` opens an editor.** macOS renders a standalone `Edit…` as `Wijzig…`
  (`Network.appex`, AppKit, verified on macOS 26.6.2, build 25G83, 2026-09-06), and this catalog reserves `Bewerk` for
  opening a file in an editor (`commands.fileEdit.label`, `menu.file.edit`). Pick by which of the two the string means.
- **A feature switched off in Settings, referenced from elsewhere, reads `… staat uit.`** The whole
  `driveIndex.tooltip*` family already says it that way, and the pointer next to it is `Zet het aan in Instellingen` /
  `Zet het aan bij <pad>`. The term `enable` (`ingeschakeld`) is for the Settings label itself.
- **Een voortgangskop staat in de infinitief, een hele zin in de werkwoordsvorm.** `Reconnecting to {name}…` wordt
  `Opnieuw verbinden met {name}…` (Apples eigen `Opnieuw verbinden…` in `ScreenSharing.loctable`, en het al aanwezige
  `errors.listing.deviceReconnecting.title`), terwijl `Try reconnecting` in een lopende zin `opnieuw verbinding maken`
  blijft, zoals de hele PPP/VPN-familie. Kies op wat de string is: een kop of een zin.
- **`Log in bij …` is het voorzetsel, en `Log in met …` de vorm voor de inlogmethode.** `servers.sheet.signInTitle`
  (`Log in bij {name}`) en `servers.sheet.signInWithCredentials` (`Log in met een gebruikersnaam en wachtwoord`) zetten
  het patroon; een nieuwe zin over inloggen hergebruikt beide in plaats van Apples `inloggen op '%@'`.
- **Android's own Dutch is Tier 1 for anything the user will read on their phone.** The AOSP catalogs are fetchable from
  `android.googlesource.com` (`packages/apps/Settings`, `frameworks/base/packages/SettingsLib`, and
  `frameworks/base/packages/SystemUI`, each `res/values-nl/strings.xml`, base64 via `?format=TEXT`), and they settle
  `USB-foutopsporing`, the `Toestaan` button, `tik op`, and the `Zet … aan` imperative. Use them the way term-choice
  principle 1 uses Apple: the word in Cmdr has to be the word on the phone's screen. Evidence: `decisions.md` § Het
  telefoonpaneel via ADB.
- **A command's register follows its ENGLISH SHAPE, not whether it toggles.** `commands.serversTogglePin.label` ("Pin /
  unpin server") keeps the slash and the imperative: `Zet server vast / maak hem los`. The two `aan/uit` commands answer
  a different English (`Toggle pin tab`, `Toggle hidden files`), which Dutch renders as `<object> <infinitive> aan/uit`;
  an English imperative takes a Dutch imperative, as all four sibling `servers.*` commands and both menu pin items
  already do. Confidence: high. Evidence: `decisions.md` § De serverhub.
- **`vastzetten` en `vast maken` zijn één werkwoordpaar, geen twee.** Beide dragen hetzelfde partikel, en het hele
  catalogus-cluster staat op `vast` / `los`: `Maak tabblad vast` / `Maak tabblad los`, `Maak vast in volumekiezer` /
  `Maak los`, `Tabblad vastzetten aan/uit`, `Vastgezet`, `Zet server vast / maak hem los`. ❌ Veeg ze dus niet samen op
  één licht werkwoord: `Maak server vast / maak hem los` herhaalt `maak` in één label. Confidence: high. Bewijs:
  `decisions.md` § De vastzet-hint.
- **"USB debugging" → `USB-foutopsporing`, settled.** It is what a Dutch Android phone shows, sourced straight from AOSP
  (`SettingsLib` `enable_adb`), and the English `@key.description` now asks for exactly that: the phrase the way the
  vendor's localized Android renders it. The `adb.*` pass reuses the same rendering. Confidence: high.
- **Three names, three jobs: `Ask Cmdr`, `Cmdr`, `de AI`.** `Ask Cmdr` now names ONLY the chat panel itself (its title,
  the View-menu item, the palette command, the Settings section, the on/off switch) and stays verbatim there. The moment
  a sentence merely describes what the AI does, it says `Cmdr` (the product acting) or `de AI` (the outside model, as in
  `suggestedOps.*`). Translate what the English says at that spot and don't re-add `Ask Cmdr` out of habit: "Wat Cmdr
  verstuurt", not "Wat Ask Cmdr verstuurt". Compounds on the panel name take a hyphen before the Dutch part:
  `het Ask Cmdr-gedeelte`, `de Ask Cmdr-instellingen`.
- **Een knoplabel dat een handeling stopt, staat in de stam-imperatief, ook als het onzichtbaar is.**
  `askCmdr.wake.stop` is de toegankelijke naam van een echte knop, dus `Stop waar Cmdr mee bezig is`, niet de infinitief
  `Stoppen …`. De knopregel hierboven geldt net zo goed voor een label dat alleen een schermlezer uitspreekt.
- **"Review" in een goedkeuringsscherm is `beoordelen`, niet `bekijken`.** `commands.suggestedOpsShow.description` opent
  een per-rij goedkeur-of-weiger-dialoog, dus `Beoordeel de bestandsbewerkingen …`, in lijn met de al vastgelegde keuze
  voor "Review file renames". De buurman `commands.logOperationLog.description` houdt `Bekijk`, want zijn Engels is "See
  a history of your file operations…": alleen kijken, geen goedkeuringsstap.
- **`Dock` blijft Engels en krijgt het lidwoord `het`.** Apple laat het lidwoord weg in korte labels
  (`Voeg toe aan Dock`, `Verwijder uit Dock`), maar zet het er in een zin wél bij (`Toon/verberg het Dock automatisch`,
  Systeeminstellingen). Kies op de vorm: label zonder, zin met. De bezittelijke vorm `je Dock` mag waar het Engels
  `your Dock` zegt. Bewijs: `decisions.md` § Het Dock-aanbod.
- **Voor een item in Cmdrs Dock-menu is `Dock.app` `nl.lproj/DockMenus.strings` de Tier-1-bron**, naast Finders
  `MenuBar.strings`. Het bestand draagt het hele Nederlandse Dock-menu op leesbare sleutels (`OPEN`, `HIDE`, `QUIT`,
  `SHOW_ALL_WINDOWS`, `KEEP_IN_DOCK`), dus een label dat het Dock zelf al kent, schrijf je niet zelf. Bewijs:
  `decisions.md` § Het Dock-menu van Cmdr zelf.
- **De map Applications heet in het Nederlandse macOS `Apps`, niet `Programma's`.** Finder gebruikt `Apps` in de
  navigatiekolom, het Ga-menu en de knopbaltip `Ga naar de map ‘Apps’`; `Programma's` leeft alleen nog in
  `Hulpprogramma's`. Schrijf `de map ‘Apps’`, met de enkele krulaanhalingstekens die deze gids voorschrijft.
- **Een Engelstalig platform levert geen Nederlands, dus het werkwoord wordt Nederlands en het woord blijft Engels.**
  GitHub stopte zijn meertalige interface op 2016-11-18 en AlternativeTo is alleen Engels, dus de knoppen die de
  gebruiker daar aanklikt heten `Star` en `Like`. Cmdr schrijft `Geef de repo een star op GitHub` en
  `Geef Cmdr een like op AlternativeTo`: het leenwoord blijft, het Nederlands draagt alleen het werkwoord. Zoek dus
  eerst uit óf het platform je taal spreekt, voordat je "gebruik de term van het platform" toepast. Bewijs:
  `decisions.md` § De onboarding-herschrijving.
- **Een `…summary`-regel naast een schakelaar mag niet omlopen**, dus die erft de terminologie van zijn lange
  `…desc`-buur maar niet diens zinsbouw: benoem in telegramstijl de kosten en de baten, en houd de regel rond de lengte
  van het Engels. Wordt het langer, snoei dan een bijwoord of een lidwoord weg, nooit een van de feiten.
- **Een `{app}`-achtige procesnaam staat vooraan zonder lidwoord, en krijgt nooit een voornaamwoord.** Een naam die Cmdr
  uit een draaiend proces leest, is een eigennaam, dus `{app} gebruikt deze schijf nog` (geen `de`/`het`), en het
  Engelse „anything **it** has open" wordt een plaatsbijwoord: `Sluit alles wat daar openstaat`. Dezelfde reden als bij
  een ongecontroleerde `{name}`: het geslacht van de naam is onbekend. Bewijs: `decisions.md` § Wie de schijf vasthoudt.
- **Een leeg inline-vakje (`<field></field>`) hoort achter het scheidbare partikel**, niet ertussenin:
  `Vul je e-mailadres in <field></field> om …`. Het partikel hoort bij zijn werkwoord; het vakje komt daarna, op
  dezelfde plek als in het Engels.
- Record case-by-case rulings here.

## Open questions

Subjective calls and coined terms that a native reviewer should confirm live in `review-queue.md`; each already ships a
reasoned value.

## Termbase files

- `../concepts.json`: the shared, language-agnostic concept registry (sense, `match` patterns, confusable neighbors).
- `terms.json`: this locale's ruling per concept, with the catalog keys that legitimately deviate under `exceptions`.
- `decisions.md`: distilled rulings, one section per feature, headings citing their keys.
- `mechanics.json`: quotes, apostrophes, spacing, and the hedge patterns `i18n-mechanics` checks.
- `review-queue.md`: open questions for a native reviewer.

Add or change a ruling in place in `terms.json` (a replaced form moves to `avoid`), and add or edit a `decisions.md`
section when the reason needs more than a line ("X over Y because Z", at most ~3 lines).
