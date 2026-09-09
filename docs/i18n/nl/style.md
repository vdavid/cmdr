# Dutch (nl) translation style guide

Working notes for translating Cmdr into Dutch. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Dutch.

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
    phrase the catalog already uses for the concept ("Op de achtergrond", like the settled "Naar prullenmand"). See the
    glossary's empty-queue button pass.
  - **A separable verb keeps its particle at the END of the label**, however far that pushes it: "Add to report" →
    `Voeg aan rapport toe`, not the English-ordered "Voeg toe aan rapport". macOS does exactly this ("Voeg aan begin van
    knoppenbalk toe", "Voeg het lettertype aan de stijl toe", verified in `nl/macOS/AppKit`, 2026-08-28). Same for
    `zet … terug`, `werp … uit`, `koppel … los`.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order is macOS (highest, Tier 1) → Microsoft (Tier 2) →
GNOME/Xfce (Tier 3). Confidence is `confirmed` (human signed off), `high` (authoritative sources agree), or `tentative`
(sources conflict or none had it).

Straightforward (sources agree, `high`):

- send → versturen (verb) / Verstuur (button) · macOS Finder ("Send"→"Verstuur", "Sending…"→"Versturen…") · high
  - Microsoft uses "verzenden"; macOS is Tier 1 and "versturen" is the native-Mac form, so prefer it.
- cancel → Annuleer (button) / annuleren · macOS ("Cancel"→"Annuleer", consistent across AppKit + Finder) · high
- copy → Kopieer (button) / kopiëren · macOS ("Copy"→"Kopieer") · high
- copied → gekopieerd (past participle) · GNOME Nautilus ("Copied …"→"… gekopieerd") · high
- show details → Toon details · macOS AppKit ("Show Details"→"Toon details") · high
- settings → Instellingen · macOS ("Settings"→"Instellingen"), MS ("settings"→"instellingen") · high
- updates → Updates (kept; capitalized as a Settings-section name) · MS ("Updates"→"Updates", ProperNoun) · high
- version → versie · MS ("version"→"versie") · high
- report → rapport · MS ("report"→"rapport") · high
- crash report → crashrapport · macOS uses "Crashrapportage" for crash reporting; "crashrapport" is the natural Dutch
  compound for the report itself · high
- quit unexpectedly → onverwachts gestopt · macOS ("unexpectedly quit"→"onverwachts gestopt") · high
- dismiss → Sluit (button) / sluiten · MS ("dismiss"→"sluiten"); rendered as the bare-stem imperative per the button
  rule · high
- done → Gereed · macOS ("Done"→"Gereed") · high
- save → bewaren · macOS ("Save"→"Bewaar"); macOS uses "bewaren", NOT "opslaan", for save · high
- file → bestand (plural bestanden) · macOS, MS, Nautilus · high
- Get Info (Finder) → Toon info · macOS Finder ("Get Info"→"Toon info") · high
- Locked (the Info-panel checkbox) → Beveiligd · macOS Finder (`InfoWindowGeneralView` `1073.title`) + AppKit
  ("Locked"→"Beveiligd") · high
- uncheck Locked → deselecteer ‘Beveiligd’ · macOS Finder `NE18`, Apple's own wording for this same recovery advice ·
  high
- eject (past participle) → uitgeworpen · from the settled eject→uitwerpen · high
- in use → in gebruik · macOS Finder ("… is in gebruik") · high
- unplug (a device) → loskoppelen ("Koppel het los") · catalog `mtp.permissionDialog.helpText`, macOS-consistent · high
- idle (a device) → niet meer bezig · counterpart of the catalog's "Het apparaat is bezig" · high
- put back (an item out of the prullenmand) → terugzetten / teruggezet · macOS Finder (`Put Back`→`Zet terug`, "could
  not be put back"→"konden niet worden teruggezet") · high
- undo, on the trash toast → Zet terug · macOS Finder's own Put Back command, which IS this action; short and
  unambiguous where `Herstel` also means Revert/Repair. The generic undo button elsewhere stays `Ongedaan maken`. See
  the glossary's prullenmandmelding pass · high
- go to trash → Ga naar prullenmand · macOS Finder (`Go to the Trash`→`Ga naar de prullenmand`), article dropped the way
  Finder's own button does (`Go to Folder…`→`Ga naar map…`) · high. NOT `Naar prullenmand`, which already means MOVE to
  trash in `fileOperations.json`
- add (to something that already exists) → toevoegen / toegevoegd; button `Voeg aan {X} toe` · macOS Finder ("Als je
  personen aan dit document wilt toevoegen …") + AppKit ("Voeg aan begin van knoppenbalk toe") · high
- note (free-text the user writes) → notitie · MS (`note`→`notitie`); settled across `errorReporter.json`. The
  feedback-pass row `note → bericht` is that surface only; don't mix the two words in one dialog · high
- the Help menu → het Help-menu · the Dutch macOS menu bar keeps `Help` (see the glossary's native-menu pass); hyphen
  after the English proper name, like `SMB-share` · high
- leave alone (Cmdr deliberately doesn't touch something) → ongemoeid laten / ongemoeid gelaten · the shipped
  `askCmdr.renameUndo.skipReason.*` family renders this exact English sentence that way, and two rollback-toast keys
  carry byte-identical English with it · high. Keep it apart from `overslaan` (`skip`), which the confirmation before
  the rollback uses. See the glossary's terugdraaimelding pass
- left where it is (a move that doesn't travel back) → laten staan · follows the settled `blijven staan`
  (`rollbackConfirm.bodyUndoByMovingBack`) · high

Add rows as terms come up, each with sources and a confidence.

From the Android (ADB) settings pass (`settings.fileOperations.adb*`; the reference pile is absent on the machine this
pass ran on, so Tier 1 was mined from the LIVE macOS bundles per `docs/i18n/reference-pile/how-to-mine.md` § "No pile on
this machine?", all verified on macOS 26.6.2, build 25G83, 2026-09-06):

- location (of a file or command on disk) → locatie · macOS Finder `Localizable.strings` (`Location`→`Locatie`,
  `Go To Location`→`Ga naar locatie`) · high
- file system → bestandssysteem · macOS AppKit `DocumentDragging.loctable` ("could not be found in the file system"→"is
  niet gevonden in het bestandssysteem") · high
- turned on (a setting that is enabled) → ingeschakeld · macOS System Settings `Localizable.loctable`, dozens of keys
  ("%@ is turned on but is not connected to a network"→"%@ is ingeschakeld, maar is niet verbonden met een netwerk") ·
  high. Prefer it over the colloquial `aanstaat`
- leave (a field) empty → laat dit leeg · macOS `Localizable.loctable` shortcut parameter help ("leave empty to preserve
  the document type"→"laat dit leeg om het documenttype te behouden"), verbatim including the `dit` · high
- installed → geïnstalleerd · macOS System Settings ("… is installed with macOS"→"… wordt samen met macOS
  geïnstalleerd") · high
- debugging → foutopsporing; USB debugging → USB-foutopsporing · AOSP's own Dutch, `SettingsLib`
  `res/values-nl/strings.xml` `enable_adb` = "USB-foutopsporing" (fetched 2026-09-07), matching Apple Dutch's
  "foutopsporingsmodus" (Safari `DeveloperPreferences.strings`). `USB` stays, the rest is localized, so the user sees
  their own phone's wording · high
- Android platform tools → Android platform tools (kept English) · the download package's own name; Google keeps it
  English in Dutch. Two words, so the Dutch compound-concatenation rule does not apply · tentative
- Android tooling (the general "you have no Android tooling installed") → Android-tools · distinct from the specific
  `platform tools` above; hyphen after the English proper name, like `SMB-share` · tentative

From the AI-copy sweep and the provider-setup pass (the app stopped calling its AI "Ask Cmdr" everywhere it just meant
"AI"; the name now survives only where it names the chat panel):

- AI features → AI-functies · the catalog itself (`settings.ai.tooltipOff` "AI-functies staan uit",
  `settings.ai.provider.description` "Kies hoe AI-functies worden aangedreven.", `onboarding.stepAi.intro`) · high
- the AI (as the acting subject, where the English deliberately doesn't say "Cmdr") → de AI · the catalog
  (`settings.askCmdr.intro` "Chat met een AI …") · high. Boundary: `Cmdr` stays `Cmdr` and `Ask Cmdr` stays
  `Ask Cmdr`; see the note below on which of the three names goes where
- AI provider → AI-aanbieder · the catalog throughout (`askCmdr.error.notConfigured`, `askCmdr.consent.intro`) · high
- file operations → bestandsbewerkingen · the catalog (`commands.operationLogShow.description`,
  `fileExplorer` "snelle bestandsbewerkingen", `settings` several) · high
- placeholder → tijdelijke aanduiding · Microsoft terminology (`DUTCH.tbx`, both senses, NLD + BEL) · high
- deployment (an Azure deployment) → implementatie · Microsoft terminology (`DUTCH.tbx`; the third sense, `inzetten`,
  is the verb) · high
- resource (an Azure resource) → resource (kept English) · Microsoft terminology (`DUTCH.tbx` keeps `resource` in
  three of four senses; only one reads `bron`) · high
- endpoint → endpoint (kept English) · **tentative, and it conflicts with Tier 2**: Microsoft terminology renders it
  `eindpunt` in seven of eight senses (`DUTCH.tbx`). The catalog keeps the loanword because the visible field label
  right above the hint is `Endpoint-URL` (`onboarding.cloudSetup.step.endpoint`), and a caption reading "Dit eindpunt"
  under a field labelled "Endpoint-URL" reads as two different things. Dutch developer usage keeps `endpoint` too.
  Sweeping both keys to `eindpunt` is the alternative; see "Decisions to confirm with David"
- in Azure (never "op Azure") · Microsoft's own Dutch style guide writes "de servers in Azure"
  (`microsoft-style-guides/StyleGuide.pdf`, its gender-neutral-writing examples) · high

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
  uitzonderingen: `glossary.md` § Native menu's.
- **The grayed-out menu item keeps its base label and adds ` (bezet)`.** A `*Busy` key is the same command in a second
  state, so it repeats its base item byte-identically and appends one marker, exactly as `menu.volume.ejectBusy`
  (`Werp uit ({name}) (bezet)`) does: `Verbreek (bezet)`, `Vergeet server (bezet)`,
  `Vergeet opgeslagen wachtwoord (bezet)`. `bezet` is the short adjectival form a parenthetical needs and it fits every
  base label unchanged; the glossary's `in use → in gebruik` is for full sentences about a volume, so don't swap one for
  the other. Never invent a second marker: if a future base label can't take `(bezet)`, report it.
- **Sentence case, not title case.** Dutch capitalizes only the first word and proper nouns, which fits the app's
  sentence-case rule directly. "Verstuur crashrapport?" not "Verstuur Crashrapport?".
- **Quotation marks:** macOS Dutch uses single curly quotes `‘…’` for quoted UI strings ("Klik op 'Ga door' …", and
  curly `‘%s’` in Nautilus). Prefer `‘…’`; avoid straight English `"…"`.
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
  so it's a judgment call (see the glossary's `osMountFallback` pass).
- **An uncontrolled `{name}` NEVER takes a pronoun.** The name can be a file (`het`) or a folder (`de`), so any pronoun
  is wrong half the time. Reach for the pronominal adverb, which works for both genders AND both numbers:
  `daar is iets aan gewijzigd`, `daar staat nu iets in`, `nadat Cmdr er klaar mee was`. Bonus: with no pronoun left, a
  `.named` and a `.counted` sibling can share one sentence, and the counted one keeps a single `plural` block instead of
  three. Worked example: `glossary.md` § De terugdraaimelding.
- **No definite article in front of a numeral.** English's "the {countText} items" has no Dutch counterpart
  (`De 1 onderdeel` is wrong), so put the completeness first and the number in an apposition after a colon:
  `Alles is teruggezet: {countText} onderdelen.` That reads correctly at every count, 1 included.
- **No reference pile on the machine? Mine the live macOS bundles, don't guess.** The `_ignored/i18n/nl/` pile lives
  only on David's laptop, so a pass running elsewhere (the M1 agent box) finds nothing at the main-clone absolute path
  either, and that is a genuinely absent pile rather than the worktree trap. Tier 1 is still reachable:
  `Finder.app`/`Safari.app` `nl.lproj/*.strings` and the framework/System Settings `*.loctable` files carry the same
  Apple Dutch, keyed by the English string or a stable id. Recipes: `docs/i18n/reference-pile/how-to-mine.md` § "No pile
  on this machine?". Tier 2 (Microsoft terminology and style guide) has no live equivalent, so a term that would have
  needed Microsoft to break a tie stays `tentative`.
- **A vendor's own Dutch UI beats a `@key.description` that says "keep it as-is",** the same way term-choice principle 1
  works for Apple. `settings.fileOperations.adbEnabled.description`'s English description asks to keep "USB debugging"
  verbatim, but a Dutch Android phone labels that switch `USB-foutopsporing`; keeping English would leave the user
  hunting for a string their phone never shows. Keep the acronym, localize the rest, and report the clash upward.
- **The volume switcher is `de volumekiezer`, in every string.** English calls one UI both "volume switcher" and "volume
  chooser"; the catalog already gives it a single Dutch name (`commands.volumeClose.label`,
  `shortcuts.scope.volumeChooser`), so a new string about it reuses that word. The glossary row `switcher → wisselaar`
  stays unused.
- **`Wijzig …` opens a form; `Bewerk …` opens an editor.** macOS renders a standalone `Edit…` as `Wijzig…`
  (`Network.appex`, AppKit, verified on macOS 26.6.2, build 25G83, 2026-09-06), and this catalog reserves `Bewerk` for
  opening a file in an editor (`commands.fileEdit.label`, `menu.file.edit`). Pick by which of the two the string means.
- **A feature switched off in Settings, referenced from elsewhere, reads `… staat uit.`** The whole
  `driveIndex.tooltip*` family already says it that way, and the pointer next to it is `Zet het aan in Instellingen` /
  `Zet het aan bij <pad>`. The glossary row `turned on → ingeschakeld` is for the Settings label itself.
- **Een voortgangskop staat in de infinitief, een hele zin in de werkwoordsvorm.** `Reconnecting to {name}…` wordt
  `Opnieuw verbinden met {name}…` (Apples eigen `Opnieuw verbinden…` in `ScreenSharing.loctable`, en het al aanwezige
  `errors.listing.deviceReconnecting.title`), terwijl `Try reconnecting` in een lopende zin `opnieuw verbinding maken`
  blijft, zoals de hele PPP/VPN-familie. Kies op wat de string is: een kop of een zin.
- **`Log in bij …` is het voorzetsel, en `Log in met …` de vorm voor de inlogmethode.** `servers.sheet.signInTitle`
  (`Log in bij {name}`) en `servers.sheet.signInWithCredentials` (`Log in met een gebruikersnaam en wachtwoord`) zetten
  het patroon; een nieuwe zin over inloggen hergebruikt beide in plaats van Apples `inloggen op '%@'`.
- **Een zin die naar de server terugwijst, herhaalt het naamwoord.** `Open de server opnieuw om …`, niet
  `Open hem opnieuw`: in een paneel waar ook `de sleutel` staat, is `hem` dubbelzinnig. De vorm komt uit
  `paneState.hostKeyChangedHint` en `fileExplorer.navigation.connectionTooltipNeedsSignIn`.
- **Android's own Dutch is Tier 1 for anything the user will read on their phone.** The AOSP catalogs are fetchable from
  `android.googlesource.com` (`packages/apps/Settings`, `frameworks/base/packages/SettingsLib`, and
  `frameworks/base/packages/SystemUI`, each `res/values-nl/strings.xml`, base64 via `?format=TEXT`), and they settle
  `USB-foutopsporing`, the `Toestaan` button, `tik op`, and the `Zet … aan` imperative. Use them the way term-choice
  principle 1 uses Apple: the word in Cmdr has to be the word on the phone's screen. Evidence: `glossary.md` § Het
  telefoonpaneel via ADB.
- **A command's register follows its ENGLISH SHAPE, not whether it toggles.** `commands.serversTogglePin.label` ("Pin /
  unpin server") keeps the slash and the imperative: `Zet server vast / maak hem los`. The two `aan/uit` commands answer
  a different English (`Toggle pin tab`, `Toggle hidden files`), which Dutch renders as `<object> <infinitive> aan/uit`;
  an English imperative takes a Dutch imperative, as all four sibling `servers.*` commands and both menu pin items
  already do. Confidence: high. Evidence: `glossary.md` § De serverhub.
- **`vastzetten` en `vast maken` zijn één werkwoordpaar, geen twee.** Beide dragen hetzelfde partikel, en het hele
  catalogus-cluster staat op `vast` / `los`: `Maak tabblad vast` / `Maak tabblad los`, `Maak vast in volumekiezer` /
  `Maak los`, `Tabblad vastzetten aan/uit`, `Vastgezet`, `Zet server vast / maak hem los`. ❌ Veeg ze dus niet samen op
  één licht werkwoord: `Maak server vast / maak hem los` herhaalt `maak` in één label. Confidence: high. Bewijs:
  `glossary.md` § De vastzet-hint.
- **"USB debugging" → `USB-foutopsporing`, settled.** It is what a Dutch Android phone shows, sourced straight from AOSP
  (`SettingsLib` `enable_adb`), and the English `@key.description` now asks for exactly that: the phrase the way the
  vendor's localized Android renders it. The `adb.*` pass reuses the same rendering. Confidence: high.
- **Three names, three jobs: `Ask Cmdr`, `Cmdr`, `de AI`.** `Ask Cmdr` now names ONLY the chat panel itself (its title,
  the View-menu item, the palette command, the Settings section, the on/off switch) and stays verbatim there. The
  moment a sentence merely describes what the AI does, it says `Cmdr` (the product acting) or `de AI` (the outside
  model, as in `suggestedOps.*`). Translate what the English says at that spot and don't re-add `Ask Cmdr` out of
  habit: "Wat Cmdr verstuurt", not "Wat Ask Cmdr verstuurt". Compounds on the panel name take a hyphen before the
  Dutch part: `het Ask Cmdr-gedeelte`, `de Ask Cmdr-instellingen`.
- **Een knoplabel dat een handeling stopt, staat in de stam-imperatief, ook als het onzichtbaar is.**
  `askCmdr.wake.stop` is de toegankelijke naam van een echte knop, dus `Stop waar Cmdr mee bezig is`, niet de
  infinitief `Stoppen …`. De knopregel hierboven geldt net zo goed voor een label dat alleen een schermlezer uitspreekt.
- **"Review" in een goedkeuringsscherm is `beoordelen`, niet `bekijken`.** `commands.suggestedOpsShow.description`
  opent een per-rij goedkeur-of-weiger-dialoog, dus `Beoordeel de bestandsbewerkingen …`, in lijn met de al vastgelegde
  keuze voor "Review file renames". De buurman `commands.operationLogShow.description` houdt `Bekijk`, want zijn Engels
  is "Browse".
- Record case-by-case rulings here.

## Decisions to confirm with David

The formality (`je`) and the send/cancel/copy terms are settled from macOS (Tier 1). Open subjective items:

- **send → versturen vs verzenden** (resolved to `versturen` from macOS, but Microsoft prefers `verzenden`): confirm
  "Verstuur rapport" reads better than "Verzend rapport" for the crash-report button. Low stakes; both are correct.
- **crash report → crashrapport** (high, but no exact macOS string for the noun): macOS has "Crashrapportage" (the
  reporting feature). Confirm "crashrapport" for the artifact reads natural in Cmdr's dialog.
- **The two new crash-dialog openings** (`crashReporter.dialog.body.keptRunning`/`.unknown`): confirm
  `een probleem tegengekomen` over the better-sourced but stiffer Tier-1 `heeft … aangetroffen`, and
  `is gewoon blijven werken` over `is gewoon actief gebleven`. Both picks trade a citation for Cmdr's warmer register.
  Evidence and the rejected alternatives: `glossary.md` § De drie crashdialoog-openingen.
- **Ask Cmdr tool-status doing/done pairs** (`askCmdr.tool.*`): no pile precedent for AI-assistant status lines, so the
  seven pairs are coined (present tense for "doing", past-participle-led for "done"; see glossary "Ask Cmdr pass" REVIEW
  FLAGS). Confirm the tone lands, and that seven distinct verbs read as a coherent family rather than ad hoc.
- **unarchive → "Uit archief halen"**: no single natural Dutch imperative verb for "unarchive" the way "Archiveer" works
  for "archive". Confirm this multi-word button reads fine next to its short siblings.
- **"Ask Cmdr model" → "Ask Cmdr-model"**: hyphenating after a two-word English brand name is a judgment call (no exact
  pile precedent for a multi-word brand + suffix). Confirm it doesn't read as awkward.
- **rename as a NOUN → "naamwijziging"** (with the compound "naamwijzigingsplan"), sourced from Microsoft's
  "naamwijzigingsvoorstellen"; the Tier-3-only "hernoeming" is out. A few keys outside the bulk-rename feature still say
  "hernoemen" (see the glossary's review flags); confirm a locale-wide sweep.
- **"Review file renames" → "Naamwijzigingen beoordelen"**: "beoordelen" (decide) over macOS's look-over "bekijken",
  because the modal is a per-row allow/deny gate. Confirm the register.
- **"No progress for {duration}" → "Al {duration} geen voortgang"**: `voortgang` is settled, but nothing in the pile
  phrases an elapsed stall, so the "al X geen Y" shape is a judgment call. Confirm it reads natural in the progress
  dialog and on a queue row.
- **"4x slower" → "4x zo langzaam als"** (`osMountFallback.message`): nothing in the pile puts a multiplier in front of
  a comparative, so the equality shape is a judgment call over the shorter, more colloquial "4x langzamer dan". Confirm
  which one reads better in a toast.
- **`errors.eject.notEjectable` avoids the macOS word for "removable"**: macOS `nl` says `Verwijderbaar`, but in a
  sentence about ejecting that reads as _deletable_ (the same collision that already ruled out `Verwijder` for eject),
  so the string says what you can do instead: "Deze schijf kun je niet uitwerpen, dus hij blijft aangesloten." Confirm
  the sidestep.
- **`errors.eject.notAnSmbVolume` repeats "verbreken" after its wrapper** ("Verbinding verbreken lukte niet: Dit is geen
  netwerkshare, dus er is geen verbinding om te verbreken."). English repeats it the same way, and Dutch `verbreken`
  needs its object, so it's deliberate. Confirm it doesn't grate in a small toast.
- **`vergrendeld` vs `beveiligd` for a locked file**: `errors.write.fileLocked.*` says `vergrendeld`, while
  `errors.mutation.fileLocked` and Apple itself say `beveiligd`. Left as-is this pass; confirm a locale-wide sweep to
  `beveiligd`. Evidence: `glossary.md` § Get Info en Beveiligd.
- **"The transfer has stopped moving" → "De overdracht komt niet meer vooruit"**: picked over the more idiomatic
  standstill phrase "ligt stil", which sits too close to the neighbouring "Gepauzeerd" state. Confirm the tradeoff.
- **"leave alone" → "ongemoeid gelaten"** (`fileOperations.cancelRollback.reason.*`, `askCmdr.renameUndo.skipReason.*`):
  nothing in the pile carries this register for files, so the shipped sibling family is the source. Confirm the register
  reads right in a toast, and that `Map {name} ongemoeid gelaten` (no article, inherited byte-for-byte from the askCmdr
  sibling the consistency check ties it to) doesn't read as clipped.
- **"after Cmdr put it there" → "nadat Cmdr er klaar mee was"**: the English place adverbial goes, so the line can stay
  gender- and number-neutral (see § Notes and decisions). Confirm the trade, and whether "De rest staat nog op de nieuwe
  plek" is the best rendering of "where the move put them".
- **"View or add notes to the report" → "Bekijk het rapport of voeg notities toe"** (`autoSentToast.viewOrAddNotes`): 39
  characters against the English 31, on a toast next to the short "Wijzig instellingen". Dutch can't hang one shared
  object in front of both verbs, so each verb carries its own half. The compact "Bekijk of vul het rapport aan" fits
  better but drops the notitie the dialog is for. Confirm which one wins in the real toast, and overflow-check it.
- **"Android file access over ADB" → "Toegang tot Android-bestanden via ADB"**: the sibling toggle above it reads
  "Ondersteuning voor Android/Kindle/camera (PTP en MTP)", so the two Android rows open with different words. Confirm
  the label pair reads as one section rather than two unrelated settings.
- **"platform tools" kept English, "Android tooling" → "Android-tools"**: no pile source (Google's Dutch pages keep the
  package name English, and the Tier-2 sources were unreachable on this machine). Confirm the split reads right, and
  that "de Android platform tools" doesn't want a hyphen.
- **"Places" → `Locaties`**: English picked `Places` over `Locations` on purpose, and Dutch has one word for both.
  Confirm `Locaties` reads right as a shortcuts-list heading next to `Volumekiezer` and `Servers`.
- **"Found nearby" → `Gevonden in de buurt`**: `in de buurt` is Apple's own rendering of `nearby`, but the word order is
  a judgment call and the label runs 20 characters in a narrow status column. Overflow-check it.
- **"Local network discovery" → `lokale netwerkdetectie`**: both halves are sourced, the compound is not. Confirm it
  over the looser `Detectie in je lokale netwerk`.
- **"there''s nothing to type" → `er valt niets te typen`** (`servers.paneState.signedOutNothingToAsk`): niets in de
  bundels draagt deze wending. `hoef je niets in te vullen` is warmer en gangbaarder, maar verschuift de zin van "er is
  niets" naar "jij hoeft niets". Bevestig welke in een klein paneel beter leest.
- **`een sleutel` naast de hostsleutel** (`servers.paneState.signedOutNothingToAsk`): dezelfde dubbelzinnigheid als het
  Engels (de sleutel van de gebruiker tegenover die van de server, waar `hostKey.*` het over heeft). De twee
  paneelstanden zijn nooit tegelijk in beeld, dus de waarde blijft letterlijk. Bevestig, of kies `een SSH-sleutel` en
  wijk daarmee van het Engels af.
- **host key → `serversleutel`** (`settings.servers.card.trustedHostKeys`, `settings.summary.servers`): gemunt, alleen
  waar het Engels zélf `host key` zegt; in het serverpaneel blijft het `de sleutel`. Bevestig dat het niet leest als
  "sleutel tót de server".
- **"Watching for phones." → `Cmdr let op telefoons.`**: het Nederlands heeft een onderwerp nodig waar het Engels er
  geen heeft. Bevestig de toon tegenover `Cmdr houdt in de gaten of er een telefoon wordt aangesloten.` (Apples idioom,
  twee keer zo lang).
- **`endpoint` vs Microsoft's `eindpunt`**: two keys carry the word, the field label
  `onboarding.cloudSetup.step.endpoint` ("Endpoint-URL") and the caption under it
  (`onboarding.cloudSetup.hint.azureEndpoint`). Microsoft terminology says `eindpunt`; both keys currently keep the
  loanword so the caption matches the label the reader can see. Confirm, or approve a two-key sweep to
  `Eindpunt-URL` / "Dit eindpunt".
- **"AI suggestions are waiting." → `Er staan AI-suggesties klaar.`** (`suggestedOps.indicatorTooltip`): nothing in the
  pile phrases waiting suggestions, so the `staan klaar` shape is a judgment call over the flatter "Er wachten
  AI-suggesties". Confirm which reads better in the status corner.
- **"Click to set one up in settings." → `Klik om er een te kiezen in instellingen.`**
  (`askCmdr.wake.needsApiKey`): the literal `in te stellen in instellingen` jangles, so the verb follows what the key's
  description says actually happens (the settings screen is where the provider is picked). Confirm the swap from
  "set up" to "kiezen".
- **"camera details" → `cameragegevens`** (`askCmdr.consent.item.contents`, `contentsRule`, `whatsNew.body`): a coined
  compound for a photo's EXIF block, no source has a collective noun for it. Confirm it reads as "what the camera
  recorded", not "data about the camera". Evidence and the fallback: `glossary.md` § Ask Cmdr looks inside files.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/nl/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
