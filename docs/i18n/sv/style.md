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
  - warning badges: noun-shaped, never an imperative (`(överskrivning!)`, not `(skriv över!)`);
  - a value spliced in after a colon doesn't repeat the frame (`errors.eject.*` after `Det gick inte att mata ut …:`).
- **Menu-bar and Apple names follow the running Swedish macOS**, looked up live and dated, never paraphrased: `Arkiv`
  (File), `Redigera`, `Innehåll` (View), `Gå`, `Fönster`, `Hjälp`, `Visa info` (Get Info), `Överblick` (Quick Look),
  `nyckelringen` / `Nyckelhanterare` (Keychain / Keychain Access), `Skivverktyg > Skivkontroll`, `Full skivtillgång`,
  `Lokalt nätverk`, `Integritet och säkerhet`, `Startobjekt och tillägg`, the folder `Appar`, the key `Retur`. Kept
  English: Finder, Spotlight, Terminal, Mission Control, System Integrity Protection, Apple silicon, and `Dock`, which
  takes no article, inflection, or possessive (`i Dock`). On a phone, Android's own Swedish wins (`USB-felsökning`,
  `Tillåt`, `tryck på`).
- **Capitalization**: sentence case; Swedish capitalizes no common nouns, days, or months.
- **Punctuation**: quote in running text with `”…”` on both sides, never straight `"…"`. A space before `%` (`100 %`,
  `{percent} %`). No comma before `och` / `eller` joining two short clauses; keep it before a consequence `så`
  (`…, så läggs det till`). Use `samt` before a last item that itself contains `och`. A command that toggles both ways
  keeps the slash (`Fäst / lossa server`).
- **Apostrophes**: ICU values double them (`''`); the RAW families (`errors.*`, `menu.*`, `licensing.windowTitle.*`,
  `main.instanceLock.*`) keep them single.
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

- **Inbyggda menyer följer Finders ordval, inte katalogens.** Där macOS har en motsvarighet vinner den, inklusive det
  överraskande `Innehåll` för View-menyn, som både Finder och Safari använder. Belägg och undantag: `decisions.md` §
  Inbyggda menyer.
- **Copy som pekar på en systemyta stavas som macOS stavar den.** Ett menyalternativ, en inställningspanel eller ett
  Apple-funktionsnamn slås upp i det KÖRANDE systemet innan det skrivs ut, och fyndet dateras. En omskrivning som "låter
  rätt" skickar användaren att leta efter något som inte finns; det var den dyraste feltypen i 2026-08-30-passet
  (`Fullständig åtkomst till skivan` mot Apples `Full skivtillgång`, `Visa > Zooma` mot `Innehåll > Zoom`). Belägg och
  listan: `decisions.md` § Apples egna namn.
- **Ett naket engelskt `All` blir `allt`, inte `alla`.** macOS `sv` säger `Markera allt` och `Avmarkera allt`; utan
  utsatt huvudord dinglar `alla` (alla vad?), medan neutrumformen står för sig själv. Gäller `Select all`,
  `Deselect all`, `Reset all to defaults`. Med huvudord böjs det normalt (`Stäng övriga flikar`). Belägg: `decisions.md`
  § Termdriftsgranskning.
- **`Hide` är `Göm`, men `hidden` är `dold`.** Apples svenska verb är `gömma` (elva `Göm …`-strängar i Finder, noll
  `Dölj`), medan `dolda filer` är den etablerade svenska filsystemtermen som Nautilus, Thunar och Total Commander delar.
  Microsofts `dölja` är Windows-konventionen och gäller inte här. Engelskans `Suppress` är ett annat verb och behåller
  `Dölj`. Belägg och nyckellista: `decisions.md` § Termdriftsgranskning.
- **Sentence case is native.** Swedish doesn't capitalize common nouns, days, or months, so the app's sentence-case rule
  applies without friction. Don't title-case.
- **Quotation marks: `”…”`** (right double quote both sides) is the standard Swedish form. Avoid English `"…"`.
- **Kommat före konsekutivt `så` står kvar.** Regeln nedan gäller `och`/`eller`, inte `så`: när andra satsen är en följd
  av den första sätter svenskan komma ("Skriv en notering eller bifoga din e-post, så läggs det till i rapporten",
  "Skicka en ny rapport från Hjälp-menyn, så når dina noteringar teamet"). Samma mening kan alltså sakna komma före
  `eller` och ha komma före `så`; det är inte inkonsekvent.
- **No comma before `och`/`eller` joining two short main clauses.** English keeps it ("Cancel it, or leave it running in
  the background"); Swedish drops it when both clauses are short ("Avbryt den eller låt den fortsätta i bakgrunden").
  Cmdr's English Oxford-comma rule is an English rule; Swedish punctuation wins here. Keep the comma only when the
  clauses are long enough that the reader needs the break. A list takes no comma before its last `och`/`eller` either.
- **Percent sign: always a space before `%`** ("100 %", "{percent} %"). Swedish typography, and what the rest of the sv
  catalog does. Don't carry English's tight `50%` across, even inside a placeholder-heavy string.
- **Warning badges are noun-shaped, not imperative.** A compact badge beside a row names a STATE, so it takes a noun
  ("(överskrivning!)"), never the imperative that would double as a command to the user ("(skriv över!)"). The
  underlying action verb (`skriv över`) is unchanged on buttons and menu items.
- **The definite form is what breaks aria containment in Swedish** (the shared rule: `../../guides/i18n-translation.md`
  § An `*Aria` key must contain its visible label). A bare indefinite label (`Bakgrund`) isn't inside the definite
  phrase a natural aria uses (`i bakgrunden`), so take the definite form for the label too (`I bakgrunden`). Swedish
  capitalizes nothing mid-sentence, so containment here is always case-insensitive. Worked example: `decisions.md` §
  Köknappen när kön är tom.
- **Numbers and dates come from the formatter layer.** Swedish uses a comma decimal and space thousands separator (1
  000), but `formatNumber()`/`formatByteSize()` produce these from the locale: never hardcode separators in a string.
- **Genitive on a brand: pick by the name's final sound.** A name ending in an s-sound (`macOS`, `iOS`) takes no
  genitive ending and no apostrophe in Swedish: "macOS inbyggda SMB-anslutning", matching the catalog's "macOS
  textstorlek". A name ending in a consonant takes the plain `-s`: "Cmdrs direktanslutning". Never write `macOS'`, and
  never the colon genitive (`pCloud:s`), which belongs to abbreviations (`SVT:s`).
- **Multipliers use `gånger`, not `x`.** English's "4x slower" becomes "fyra gånger långsammare"; the spell-out rule
  (one through nine as words, 10+ as digits) applies inside the multiplier, so "fyra gånger" but "100 gånger".
- **Syskonvarianter av samma mening delar ram.** När engelskan delar en nyckel i flera varianter som fyller samma plats
  i samma dialog (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`), ska den delade delen vara identisk
  tecken för tecken och subjektet vara detsamma i alla varianter. Det får väga tyngre än en enskild belagd formulering:
  en variant som byter konstruktion läses som en annan mening, inte som samma mening med ett annat innehåll. Skriv om
  alla varianter samtidigt eller ingen. Belägg och det avvisade alternativet: `decisions.md` § Kraschdialogens tre
  varianter.
- **Ett värde som hamnar efter kolon upprepar inte ramen.** `errors.eject.*` matas in i
  `fileExplorer.pane.ejectFailedToast` ("Det gick inte att mata ut {volumeName}: …") och `.disconnectFailedToast`. Ramen
  har redan sagt att det inte hände, så värdet säger bara varför och vad man gör åt det; inget "det gick inte att" en
  gång till, och ingen inledande versal-mening som läser som en ny rubrik. Belägg och de nio värdena: `decisions.md` §
  Utmatning och frånkoppling.
- **`koppla från`, `koppla ur` och `dra ur` är tre olika saker.** Programmässig frånkoppling, enheten ur porten, kabeln
  ur uttaget. Engelskan har `disconnect` och `unplug`; svenskan skiljer dem tydligare, så välj efter vad som faktiskt
  händer. Belägg: `decisions.md` § Utmatning och frånkoppling.
- **`samt` före sista ledet när det ledet själva innehåller `och`.** Engelskans Oxford-komma bär upp ”…, an archive, and
  a photo’s camera details and location”; svenskan har inget serie-komma, och två `och` i rad med olika räckvidd blir
  otydligt. Byt det yttre `och` mot `samt`: ”…, vad som finns i ett arkiv samt en bilds kamerauppgifter och var den
  togs”. Belägg: `decisions.md` § Ask Cmdr tittar in i filer.
- **`plats` är var en fil ligger; var en bild togs skrivs ut som bisats.** `plats` är det satta ordet för `location` i
  filsystemsmening, så direkt efter ”en bilds” läses det som filens plats. Skriv `var den togs` när engelskan menar
  fotots geoposition. Belägg: `decisions.md` § Ask Cmdr tittar in i filer.
- **Ett `eller` inuti ett led som redan hänger på ett `eller` klaras av kommat, inte av ett nytt ord.** Engelskans ”Add
  one below, or turn on a Mac or NAS on your network…” har två `or` med olika räckvidd. Svenskan har ingen `samt`-utväg
  här (den gäller `och`), så det yttre `eller` markeras med komma före sig och det inre lämnas naket: ”Lägg till en
  nedan, eller slå på en Mac eller NAS i ditt nätverk, så hittar Cmdr den” (`servers.hub.emptyMessage`). Artikeln delas
  av båda leden, eftersom `Mac` och `NAS` båda är en-genus.
- **Ett kommando som packar båda riktningarna behåller engelskans snedstreck.** `commands.serversTogglePin.label` (”Pin
  / unpin server”) blir `Fäst / lossa server`, inte en `eller`-form. Katalogens `eller` hör till de strängar där
  engelskan själv skriver ”or” (`commands.viewShowHidden.label`), så snedstrecket bär informationen att det är ETT
  kommando som växlar. Belägg för verben: `decisions.md` § Serverhubben: tabellen.
- **Statuscellerna i en tabell är particip som böjs efter radens huvudord.** Serverhubbens rader är `en server`, alltså
  en-genus: `Ansluten`, `Sparad`, `Hittad i närheten`, `Utloggad`. Slår raden om till ett neutrumord någon gång måste
  hela kolumnen skrivas om, inte bara den nya statusen.
- **En ”X again to retry”-mening tar `igen` en gång och `på nytt` för retry-ledet.** Engelskan kan upprepa `again`,
  svenskan blir tramsig på `igen … igen`. Katalogens formel är `<handling> igen för att försöka på nytt` (ett dussin
  `errors.listing.*.suggestion` säger ”Gå hit igen för att försöka på nytt”), och
  `servers.paneState.signedOutNothingToAsk` följer den: ”Öppna servern igen för att försöka på nytt.” `Försök igen` står
  kvar som knapptext för `Try again`, och `prova` hör till `try` i betydelsen testa något.
- **`Ask Cmdr` står kvar bara där engelskan har kvar det, och där namnger det bara CHATTPANELEN.** I övrigt är subjektet
  `Cmdr` eller `AI:n`, efter vad nyckelns engelska säger; `AI features` blir `AI-funktioner`. Översätt aldrig efter
  minnet av hur nyckeln såg ut förut, läs den aktuella engelskan. Termer, belägg och skillnaden mellan `chatt` och
  `samtal`: `decisions.md` § Ask Cmdr namnger bara chattpanelen.
- **En engelsk term som katalogen redan har ett settlat ord för vinner över en engångsformulering.**
  `onboarding.cloudSetup.step.install` var enda stället som sa `Ladda ner` mot 43 `hämta` i resten av katalogen; det var
  drift, inte ett val. Kolla alltid hur ofta en form redan förekommer innan du skriver en ny. Belägg: `decisions.md` §
  Stegen för att sätta upp en AI-leverantör.
- **`Dock` är Apples yta, så `fäst`/`lossa` gäller inte där.** Katalogens pin/unpin-par hör till Cmdrs egna ytor
  (flikar, servrar); i Dock skriver macOS `Behåll i Dock` / `Ta bort från Dock`, och `Dock` står oböjt utan artikel och
  utan possessiv (`i Dock`, inte `i Docken` eller `i din Dock`). Belägg: `decisions.md` § Dock-erbjudandet.
- **Dockmenyns egna ord slås upp i Dock, inte i Finder.** Referenssamlingen bär inte Dock, så
  `Dock.app/Contents/Resources/sv.lproj/DockMenus.strings` är Tier 1 för högerklicksmenyn på appsymbolen. Den skiljer på
  APPNAMNSformen (`Göm %@`, `Visa %@`, naket namn) och FILNAMNSformen (`Öppna ”%@”`, svenska citattecken), så
  `Open Cmdr` blir `Öppna Cmdr` utan citattecken. Belägg: `decisions.md` § Dockmenyn.
- **Mappen `Applications` heter `Appar` sedan macOS 26, inte `Program`.** Finder, AppKit och Go-menyn säger alla
  ”Appar”, och dra-meningen skrivs `dra … från mappen Appar`. Belägg: `decisions.md` § Dock-erbjudandet.
- **En tom tagg som renderar en kontroll mitt i meningen sätts där svenskan vill ha objektet.**
  `onboarding.stepBeta.checklist.email` har `<field></field>`, som är textfältet plus dess Spara-knapp inuti satsen.
  Meningen måste läsas som EN rad med en ruta i mitten, alltså placeras taggen efter objektet (”Ange din e-postadress
  <field></field>, så …”), inte i engelskans position. Samma reflex gäller `<alpha></alpha>` och `<chip></chip>`.
  Belägg: `decisions.md` § Introduktionsguidens omskrivning.
- **När en sträng CITERAR en annan nyckels etikett är de två en enhet, precis som ett `*Aria`-par.**
  `onboarding.stepAi.local.tooltip` säger `<strong>Ja, jag vill ha AI</strong>` och pekar därmed på
  `onboarding.stepAi.cloud.label`, som måste stå ordagrant likadant; `signup.rejected`/`.unreachable` citerar
  `checklist.emailSave` (`Spara`) på samma sätt. Skriv om båda samtidigt eller ingen.
- **En systembehörighets namn hämtas från Apple, ordagrant, aldrig som parafras.** Både sammanfattningen bredvid
  nätverksströmbrytaren och `…networking.desc` säger `Lokalt nätverk`, Apples egen etikett, inte den beskrivande
  `Lokal nätverksåtkomst`. Ett namn användaren inte hittar i Systeminställningar är samma feltyp som
  `fullständig åtkomst till skivan` var. Belägg och den live lästa bunten: `decisions.md` § Introduktionsguidens
  omskrivning.
- **Referenssamlingen bär falska vänner; kolla vad strängen sitter bland innan du lånar den.** Total Commanders
  `Skrivfel!` ser ut som `typo` men är `Write error` (den ligger bland filoperationsfelen). En träff på rätt svenskt ord
  är inte belägg förrän källans egen betydelse stämmer.
- **GitHub och AlternativeTo har inget svenskt gränssnitt att kopiera.** GitHub lade ner sin UI-lokalisering 2016-11-18
  (svenska fanns 2010–2016), och AlternativeTo är helt engelskt. Deras verb översätts alltså som vanliga termer, inte
  som citerade knappetiketter: `star` → `stjärnmärk` (katalogens egen precedens), `Like` → `Gilla` (Microsoft sv).
  Belägg: `decisions.md` § Introduktionsguidens omskrivning.
- Record case-by-case rulings in `terms.json` and `decisions.md` so they aren't relitigated.

## Open questions

Subjective calls and coined terms that a native reviewer should confirm live in `review-queue.md`; each already ships a
reasoned value.

## Termbase files

- `../concepts.json`: the shared, language-agnostic concept registry (sense, `match` patterns, confusable neighbors).
- `concepts-proposed.json`: concepts this locale needed that the shared registry doesn't have yet, merged into
  `../concepts.json` afterwards.
- `terms.json`: this locale's ruling per concept, with the catalog keys that legitimately deviate under `exceptions`.
- `decisions.md`: the rationale journal, one section per feature, headings citing their keys.
- `review-queue.md`: open questions for a native reviewer.

Add or change a ruling in place in `terms.json` (a replaced form moves to `avoid`), and add a `decisions.md` section
when the reason needs more than a line. Source every term from the reference pile (`_ignored/i18n/sv/`; recipes in
`../reference-pile/how-to-mine.md`) or the live macOS bundles, and never guess one.
