# Swedish (sv) translation style guide

Working notes for translating Cmdr into Swedish. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Swedish. Term rulings live in `terms.json`
(keyed by the shared `../concepts.json`), their rationale in `decisions.md`, and open questions in `review-queue.md`.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address**: informal `du`, lowercase, everywhere, like macOS Swedish and Microsoft's style guide. Never `ni`.
- **Voice**: friendly, concise, active, calm. Error copy states the problem and a next step and never labels the event
  with `fel` or `misslyckades`: body `Det gick inte att …`, clipped headline and status `Gick inte att slutföra …`,
  "Couldn't X" with a subject `Cmdr kunde inte …`, "Something went wrong" `Något gick fel` (the collocation is fine).
  Prefer an active sentence to the passive `-s` (`Ta bort filen`, not `Filen tas bort`).
- **Register by UI slot**:
  - buttons and menu items: imperative (`Spara`, `Avbryt`, `Radera`, `Byt namn`, `Kopiera`, `Flytta`);
  - titles and yes/no questions: imperative or bare infinitive plus `?` (`Avsluta medan en åtgärd pågår?`);
  - progress and queue rows: finite present tense, no subject (`Kopierar`, `Flyttar`, `Söker igenom…`);
  - status cells and chips: a participle agreeing with the row's noun (`Ansluten`, `Sparad`, `Pausad`, `Ångrad`);
  - warning badges: noun-shaped, never an imperative (`(överskrivning!)`, not `(skriv över!)`).
- **Menu-bar and Apple names follow the running Swedish macOS**, looked up live and dated, never paraphrased: `Arkiv`
  (File), `Redigera`, `Innehåll` (View), `Gå`, `Fönster`, `Hjälp`, `Visa info` (Get Info), `Överblick` (Quick Look),
  `nyckelringen` / `Nyckelhanterare` (Keychain / Keychain Access), `Skivverktyg > Skivkontroll`, `Full skivtillgång`,
  `Lokalt nätverk`, `Integritet och säkerhet`, `Startobjekt och tillägg`, the folder `Appar`, the key `Retur`. Kept
  English: Finder, Spotlight, Terminal, Mission Control, System Integrity Protection, Apple silicon, and `Dock`, which
  takes no article, inflection, or possessive (`i Dock`). On a phone, Android's own Swedish wins (`USB-felsökning`,
  `Tillåt`, `tryck på`).
- **Capitalization**: sentence case; Swedish capitalizes no common nouns, days, or months.
- **Typography** (`mechanics.json`): quotes `”…”` with the closing mark on both sides, nested `’…’`, apostrophe `’`;
  never straight `"` or English `“…”`. Ellipsis `…`. A space before `%` (`100 %`, `{percent} %`). Write any apostrophe
  as the curly `’`, which needs no ICU doubling in any family.
- **No hedged grammar**: never `fil(er)`, `mapp(en)`, `markerad/-t`, `en/ett`, `den/det`, or an ending glued to an
  insert (`{name}s`, `{name}:s`, `{system_settings}en`). Use ICU plural / select when Cmdr knows the value; otherwise
  name the noun (`filen`, `objektet`), use a preposition (`på {name}`), or put the insert after a colon.
- **Punctuation**: no comma before `och` / `eller` joining two short clauses; keep it before a consequence `så`
  (`…, så läggs det till`). Use `samt` before a last item that itself contains `och`. A command that toggles both ways
  keeps the slash (`Fäst / lossa server`).
- **Everyday words in prose**: `bara` over `endast`, `det här` over `detta`, `ansluta direkt` over
  `upprätta en anslutning`, `ställa in` over `konfigurera` in running text. Names keep their ruled form.
- **Compounds** close up (`fillista`, `åtgärdskö`) and hyphenate after an acronym, a code, or a proper name
  (`API-nyckel`, `zip-arkiv`, `USB-enhet`, `Finder-taggen`, `Dock-inställningar`, `Hjälp-menyn`); a two-word phrase
  can't compound, so use a phrase instead.
- **Brand genitive by final sound**: a consonant takes a plain `-s` (`Cmdrs`, `Finders`, `pClouds`), an s-sound takes
  nothing (`macOS inbyggda …`). Never `macOS'` or `pCloud:s`.
- **Placeholders**: never inflect or put a genitive on an uncontrolled `{name}`, `{host}`, `{volumeName}`: use a
  preposition (`nyckeln från {host}`, `på {volumeName}`, `som heter {folderName}`) or a direction adverb (`dit`). Where
  `den` could point two ways, repeat the noun (`Öppna servern igen`). Cmdr's own behavior repeats `Cmdr` rather than a
  pronoun.
- **Plurals**: CLDR `one` / `other`. Keep `en`/`ett` agreement inside each branch (`en markerad fil`,
  `ett markerat objekt`); pull a tail that agrees with the counted noun into both branches; a counted `*Text` with no
  selector takes the invariant `objekt`; "the {countText} items" becomes `allt …: {countText} objekt`.
- **Numbers**: one through nine as words, 10+ as digits, also in multipliers (`fyra gånger`, `100 gånger`, never `4x`).
  Separators and decimals come from the formatter layer.
- **Top traps** (details in `terms.json`):
  - operation → `åtgärd` (queue `Åtgärdskö`, log `Åtgärdslogg`); transfer → `överföring` only for a copy/move in flight.
  - delete files → `Radera`; `Ta bort` only takes something out of a list; move to trash → `Flytta till papperskorgen`.
  - rollback → `ångra` (`Ångra klart`), never `återställ` (that's restore; names come back with `återställa`); put back
    from the trash → `Lägg tillbaka`; stopping a rollback → `Stoppa`, never `Avbryt`.
  - dismiss → `Avfärda`; close → `Stäng`; cancel → `Avbryt`.
  - hide → `Göm`, hidden → `dold`; only English Suppress keeps `Dölj`.
  - select files → `Markera` (`Markera allt`, never `alla`); pick an option → `Välj`; the set → `markeringen`.
  - share → `delad mapp` (`-resurs` only inside a compound, `delning` only in the counter); photo → `bild`.
  - download and fetch → `hämta`, never `ladda ner`; the Downloads folder → `Hämtade filer`.
  - disconnect (software) `koppla från`, unplug the device `koppla ur`, pull the cable `dra ur`; eject → `mata ut`.
  - turn a feature on in a hint → `Slå på`; a Settings switch and a license → `aktivera`; off → `är avstängd`.
  - drive → `enhet`, English disk → `disk`, `skiva` only in Finder's own wording; item → `objekt`; pane → `panel`.
  - Running: an operation `Pågår`, a process `Körs`. Checking: verifying `Kontrollerar`, looking for `Söker efter`.
  - `Ask Cmdr` names only the chat panel; prose about the AI says `Cmdr` or `AI:n`.

## Formality

- **`du`, lowercase, throughout. No `ni`.** Settled, high: Microsoft's Swedish style guide says "Rewrite to use the
  second person (du)" and macOS Finder addresses the user with `du` ("Du kan inte ångra den här åtgärden").
- **Buttons and menu items: imperative verb.** "Spara", "Avbryt", "Radera", "Byt namn", "Kopiera", "Flytta". This is the
  macOS/Windows Swedish norm.
- Avoid the passive-`-s` where an active imperative reads better ("Ta bort filen", not "Filen tas bort").

## Voice and tone

Friendly, concise, active, calm. Swedish OS software is already informal and direct, so Cmdr's English voice carries
over cleanly. Keep sentences short and natural-spoken, not bureaucratic. Error messages stay calm and actionable and
never say "fel" (error) or "misslyckades" (failed) as a label the way English avoids "error"/"failed": phrase the
problem and the next step ("Det gick inte att byta namn på filen. Försök igen?"), not a status code.

## Decision points

The localization calls for Swedish beyond formality. Swedish has no script, gender-agreement, or RTL complications, so
the surface is small. Evidence verified against the reference pile (`_ignored/i18n/sv/`) on 2026-06-20.

- **Regional variant: one `sv` catalog targeting Sweden-Swedish, no separate `sv-FI`.** Microsoft and Apple both ship a
  single Swedish UI, as do Google, Spotify, and Netflix, and let the formatter handle regional number, date, and
  currency differences. The Finland-Swedish differences that matter to Cmdr already come from `formatNumber()` /
  `formatByteSize()`, not from catalog strings. Revisit only if Cmdr wants a deliberately Finland-Swedish presence.
- **Gender and inclusive language: a non-issue in Swedish UI.** Swedish UI strings don't gender the user. The one live
  point is `en`/`ett` noun gender driving article and adjective agreement inside plural and count branches (see
  Plurals).
- **Script, capitalization, length: no special handling.** Latin script, no RTL, native sentence case. Length runs close
  to English, so overflow risk is lower than German, but still overflow-check against the pseudolocale (`en-XA`).

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json` (and `concepts-proposed.json`
until those are merged): `chosen`, accepted forms, usage notes, forms to avoid with the reason, a confidence
(`confirmed` / `high` / `tentative`), and sources. Swedish IT terminology follows Svenska datatermgruppen and
Apple/Microsoft Swedish; prefer the macOS term when macOS and Windows differ, since Cmdr is a macOS app. Sources are
read but never copied verbatim (Apple/MS copyrighted, GNOME/Xfce GPL): they decide the term, Cmdr writes its own value.
Rationale worth more than a line sits in `decisions.md` under a heading that cites its keys. Never guess a term: mine
the reference pile first (`../reference-pile/how-to-mine.md`).

### Busy (disabled) menu items

A menu item that's grayed out because a copy or move still needs the volume or server keeps its normal wording and adds
` (upptagen)` at the end, nothing else. `menu.volume.ejectBusy` set the precedent ("Mata ut ({name}) (upptagen)") and
`menu.volume.disconnectBusy` / `.forgetSavedPasswordBusy` / `.forgetServerBusy` follow it: "Koppla från (upptagen)",
"Glöm sparat lösenord (upptagen)", "Glöm servern (upptagen)". One marker for the whole catalog; don't invent a second
one, and don't reword the base item to fit it. `upptagen` stays uninflected here: it's a parenthetical status tag on the
action, not an adjective agreeing with a noun in the label, and the thing that's busy (servern, volymen, enheten) is
`en`-gender anyway. `high` (macOS Finder grays such items out without a marker, so the parenthetical is Cmdr's own; the
word itself is the standard Swedish "busy"). A disabled BUTTON's tooltip doesn't carry it:
`fileExplorer.navigation.disconnectBusyTooltip` says
`Det går inte att koppla från medan åtgärder pågår på den här servern`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, plus the `{system_settings}`-style tokens. The
curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`. Quick Look is NOT on it: Apple translates it to `Överblick`. The macOS UI
names Cmdr opens into (System Settings panes, "Papperskorgen") should match what a Swedish macOS actually shows. Never
hang a suffix or preposition off a system token: write `i {system_settings}`, never `{system_settings}en`.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('sv')`). Write both branches.

- Swedish plural form depends on the noun's declension and isn't a simple "+r": "1 fil" / "2 filer", but "1 objekt" / "2
  objekt" (neuter nouns ending in a consonant often don't change). Write the natural plural for each noun, don't
  pattern-match off English.
- `en`/`ett` gender affects agreement ("en markerad fil" vs "ett markerat objekt"). Keep article and adjective agreeing
  with the counted noun inside each branch.
- **Ett räknat `*Text`-tal UTAN egen plural-väljare behöver ett oböjligt substantiv.** Engelskan klarar sig med bara
  talet, men svenskan vill ha ett räknat huvudord, och då finns ingen heltalsplaceholder att välja form på. Nödlösningen
  är ett neutrumord som ser likadant ut i singular och plural: `objekt`. Be hellre om en heltalspartner:
  `fileOperations.trash.undonePartial` fick en `{skipped}`-väljare just av det skälet, så andra satsen är nu en vanlig
  plural (`{skippedText} {skipped, plural, one {objekt} other {objekt}} ligger kvar i papperskorgen`). Lägg aldrig en
  egen `{n, plural, …}` runt själva `*Text`-värdet: det är en färdigformaterad sträng, inte ett tal.
- **Pull a shared tail INSIDE the branches when it agrees with the counted noun.** English often leaves a trailing
  clause outside the plural ("{count, plural, …} and may already be partly written"); Swedish predicative adjectives and
  participles inflect for number ("öppen … skriven" vs "öppna … skrivna"), so a shared tail is wrong in one branch.
  Duplicate the tail into `one` and `other` instead. The placeholder set stays identical, so parity still passes
  (`fileOperations.transferProgress.stallInFlight` is the worked example).
- **Bestämd totalitet framför ett `*Text`-tal skrivs om till `allt`.** Engelskan markerar med `the` att det var ALLT
  ("Removed the {countText} items"), men svenskan kan inte sätta artikel framför en färdigformaterad talsträng:
  `de 1 objekt` blir fel i `one`-grenen, och `det enda objektet` tappar `{countText}`, som måste stå i båda grenarna.
  Skriv i stället `allt` plus kolon och antalet: `Raderade allt Cmdr hade skrivit: {countText} objekt` mot den partiella
  systersträngens `Raderade {countText} objekt`. Kontrasten mellan hel och delvis ångring överlever, och båda grenarna
  blir grammatiska (`fileOperations.cancelRollback.doneDeleting`/`.someDeleted` är det utskrivna exemplet).

## Notes and decisions

- **Sentence case is native.** Swedish capitalizes no common nouns, days, or months. Don't title-case.
- **Quotation marks**: `”…”` with the closing mark on both sides, nested `’…’` the same way, apostrophe `’`
  (`mechanics.json`). Never straight `"…"` or English `“…”`.
- **Commas**: no comma before `och` / `eller` joining two short main clauses, nor before a list's last item; keep it
  when the clauses are long enough to need the break, and always before a consequence `så` ("Skriv en notering eller
  bifoga din e-post, så läggs det till …"). An `eller` inside an item already hanging on an `eller` is marked by a comma
  before the outer one (`servers.hub.emptyMessage`). Use `samt` before a last item that itself contains `och`.
- **Percent**: a space before `%` (`100 %`, `{percent} %`), even inside a placeholder-heavy string.
- **Numbers and dates** come from the formatter layer (comma decimal, space thousands separator); never hardcode them.
- **Multipliers** use `gånger`, spelled out through nine: `fyra gånger`, `100 gånger`, never `4x`.
- **Brand genitive by final sound**: `Cmdrs`, `Finders`, `pClouds`; an s-sound takes nothing (`macOS inbyggda …`). Never
  `macOS'`, never the abbreviation colon form (`pCloud:s`; that's for `SVT:s`).
- **Warning badges are nouns** (`(överskrivning!)`), never an imperative that doubles as a command (`(skriv över!)`).
- **The definite form breaks aria containment**: a bare indefinite label (`Bakgrund`) isn't inside the definite phrase a
  natural aria uses (`i bakgrunden`), so the label takes the definite form too (`I bakgrunden`). Containment is always
  case-insensitive here, since Swedish capitalizes nothing mid-sentence.
- **Table status cells are participles agreeing with the row's noun** (`servern` → `Ansluten`, `Sparad`); if the row
  noun changes gender, rewrite the whole column.
- **"X again to retry"** takes `igen` once and `på nytt` for the retry (`Gå hit igen för att försöka på nytt`), since
  `igen … igen` reads silly. `Försök igen` is the Try again button; `prova` is try-out.
- **Three unplugging verbs**: `koppla från` (software), `koppla ur` (the device out of the port), `dra ur` (the cable).
- **A toggle command keeps the slash** (`Fäst / lossa server`): it's ONE command; an `eller` form belongs to strings
  whose English itself says "or".
- **Look up Apple names in the running macOS**, never paraphrase them: menu items, System Settings panes, the
  Applications folder (`Appar` since macOS 26), the Dock (`i Dock`, pin / unpin `Behåll i Dock` / `Ta bort från Dock`).
  `decisions.md` § Apple names are looked up live, § Native menus, § Dock offer.
- **An empty tag that renders a control mid-sentence** (`<field></field>`, `<alpha></alpha>`, `<chip></chip>`) goes
  where Swedish wants the object, not where English put it.
- **GitHub and AlternativeTo have no Swedish UI**, so their verbs translate (`stjärnmärk`, `Gilla`); Android's own
  Swedish is quoted verbatim (`USB-felsökning`, `Tillåt`).

## Open questions

Subjective calls and coined terms that a native reviewer should confirm live in `review-queue.md`; each already ships a
reasoned value.

## Termbase files

- `../concepts.json`: the shared, language-agnostic concept registry (sense, `match` patterns, confusable neighbors).
- `concepts-proposed.json`: concepts this locale needed that the shared registry doesn't have yet, merged into
  `../concepts.json` afterwards.
- `terms.json`: this locale's ruling per concept, with the catalog keys that legitimately deviate under `exceptions`.
- `decisions.md`: distilled rulings ("X over Y because Z"), headings citing their keys, edited in place.
- `mechanics.json`: quotes, apostrophe, spacing, and hedge patterns, checked by `i18n-mechanics`.
- `review-queue.md`: open questions for a native reviewer.

Add or change a ruling in place in `terms.json` (a replaced form moves to `avoid`), and add a `decisions.md` section
when the reason needs more than a line. Source every term from the reference pile (`_ignored/i18n/sv/`; recipes in
`../reference-pile/how-to-mine.md`) or the live macOS bundles, and never guess one.
