# Serbian (sr) translation style guide

Working notes for translating Cmdr into Serbian. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Serbian.

Serbian is the hard one. It's digraphic: the same language is written in both Cyrillic and Latin, fully 1:1
interchangeable, and tech UIs split on which to default to. That script call is the central decision below and it's
David's to make. Everything else (variant, formality, gender, plurals) resolves cleanly from the reference pile.

## Decisions to confirm with David

These need David, not a translator:

- **Script: Cyrillic vs Latin (the big one).** A product and brand-identity call, not a linguistic one. Both scripts are
  correct Serbian and the pile proves they're mechanically interchangeable (see the script decision point below for the
  evidence and a recommendation). Pick one as the shipped default; optionally ship both later as `sr-Cyrl` and `sr-Latn`.
- **Catalog tag: don't ship bare `sr`.** Whatever script David picks, target an explicit script subtag for the catalog
  directory (`sr-Cyrl` or `sr-Latn`), mirroring the reference pile's split. Bare `sr` is script-ambiguous and leaves the
  rendering to the platform's guess. `tentative` only on which subtag, `high` that it should be explicit.

Everything below assumes those are settled; the term and grammar choices hold for either script (transliterate the
glossary values when the script flips).

## Voice and tone

Friendly, concise, active, calm. The Microsoft Serbian voice guidance lines up with Cmdr's: "warm and relaxed, less
formal", "crisp and clear", everyday conversational words over formal/technical register (verified against the reference
pile, 2026-06-20). That carries Cmdr's English voice over cleanly.

Error messages stay calm and actionable. Serbian has no single noun as loaded as English "error"; still, frame the
problem and the next step rather than labeling a failure. Prefer the impersonal "Није могуће преименовати датотеку."
(it wasn't possible to rename the file) over a blunt "Грешка" (error) heading. Don't open with "Грешка"/"Greška" or
"Није успело"/"Nije uspelo" as a status label the way English avoids "error"/"failed".

## Formality

**Verdict: informal `ti`, not the polite plural `Vi`.** Consumer brands (IKEA, Spotify, Netflix, and peers; IKEA-RS
uses informal `ti`/`tvoj`) address Serbian users informally, which fits Cmdr's friendly personal voice. Formality
decision recorded in [`formal-informal-decisions.md`](formal-informal-decisions.md).

- **Direct address: informal singular `ti`** ("кликни", "изабери" / "klikni", "izaberi"), not the polite plural `Vi`
  ("кликните", "изаберите") that the OS sources lean on. Cmdr deliberately picks the warmer consumer-brand register.
  `high`.
- **Buttons and menu items: bare imperative.** GNOME and Microsoft render action labels as imperatives: "Премести у
  смеће" / "Premesti u smeće" (Move to trash), "Преименуј" / "Preimenuj" (Rename), "Откажи" / "Otkaži" (Cancel)
  (verified against the reference pile, 2026-06-20). A bare-imperative label is an action name, not address, so it sits
  fine under a `ti` register; full sentences addressing the user take singular `ti`.
- **Sentence case, not title case.** Serbian doesn't capitalize common nouns; the app's sentence-case rule applies
  natively. Capitalize only the first word and proper names.

## Decision points

### Script: Cyrillic vs Latin (central, David-only)

- **The fact.** Serbian is written in both Cyrillic (ćirilica) and Latin (latinica), and the two map 1:1, character for
  character. The reference pile shows it directly: GNOME's Cyrillic catalog says "Премести у смеће", "Нова фасцикла",
  "Преименујем", and the Latin catalog says the byte-for-byte transliteration "Premesti u smeće", "Nova fascikla",
  "Preimenujem" (verified against the reference pile, 2026-06-20). Choosing a script is not choosing words; it's choosing
  a font of letters. So this is a brand and audience call, not a translation one.
- **The tension.** Serbia's constitution and official/government use mandate Cyrillic, and it carries cultural and
  national weight. But everyday digital life, the web, messaging, and most consumer tech UIs lean heavily Latin, because
  Latin is keyboard-default, diaspora-readable, and cross-Yugoslav. A tech product that defaults to Cyrillic reads as
  "official/traditional"; one that defaults to Latin reads as "modern/casual/regional". Cmdr's voice is the latter.
- **How the majors handle it (concrete):**
  - **Apple / macOS: ships no Serbian at all.** There is no `macOS/` folder for any Serbian variant in the pile (every
    well-supported language has one); Serbian isn't in macOS's system UI language list. A Serbian Mac user runs the OS in
    English or another language. So there's no Apple precedent to match and no platform Serbian Finder string to mirror,
    unlike every other language guide here (verified against the reference pile, 2026-06-20). This is a notable gap: the
    usual Tier-1 macOS authority is simply absent for Serbian.
  - **Microsoft / Windows: ships three Serbian locales, `sr-Cyrl-RS`, `sr-Latn-RS`, and `sr-Cyrl-BA`,** with full
    terminology and style guides per script (the pile has both `sr-Cyrl` and `sr-Latn` terminology TBX plus three style
    guides). Windows lets the user pick the script as a distinct language choice; there's no single "Serbian", you
    install the script you want. So Microsoft treats the scripts as parallel first-class locales rather than defaulting
    one (verified against the reference pile, 2026-06-20).
  - **Google (Android, Chrome, Search):** offers both and defaults to Latin (`sr-Latn`) for Serbian in most consumer
    surfaces; Cyrillic is available as an explicit choice. Google's CLDR (which Cmdr's `Intl` formatting rides on) treats
    `sr` as primarily Latin in many tools' defaults.
  - **Spotify, Netflix, and most consumer web/app UIs that localize Serbian at all: Latin.** When these ship a Serbian
    interface it's overwhelmingly Latin script, matching everyday web usage. Many don't ship Serbian and fall back to
    English. (Cross-checked as industry pattern, not in the pile; treat as `tentative` evidence, but it aligns with the
    everyday-Latin lean the pile and CLDR show.)
- **Recommendation: default to Latin (`sr-Latn`), with Cyrillic (`sr-Cyrl`) as a strong second locale to add later.**
  Confidence `tentative` and explicitly David's call. Reasoning: Cmdr is a modern, casual macOS dev/power-user tool; its
  audience (developers, file power-users, diaspora) skews to the everyday-Latin world, and Latin matches how Google and
  most consumer tech default. Latin also keeps the door open to Croatian/Bosnian/Montenegrin readers. If David wants to
  signal respect for official Serbian or target a Serbia-first cultural identity, Cyrillic is the equally-correct
  alternative, and because the scripts are 1:1, shipping the second one later is transliteration, not retranslation.
- **Tag guidance: ship an explicit script subtag, never bare `sr`.** Use `sr-Latn` (or `sr-Cyrl`) for the catalog
  directory so the rendering is deterministic and matches the pile's convention. `high` on "explicit subtag", the
  subtag itself follows the script call above.

### Regional variant and pronunciation: ekavian vs ijekavian

- **The fact.** Serbian has two standard reflexes of the old "yat" vowel: ekavian (Serbia, the dominant standard digital
  Serbian, "e": "Премештено" / "Premešteno") and ijekavian (Bosnia, Montenegro, parts of Croatia, "(i)je":
  "премјешта"). The pile's `sr-ije` GNOME catalog is the ijekavian variant and shows the "je" reflex where plain `sr`
  shows "e" (verified against the reference pile, 2026-06-20).
- **How the majors handle it.** Microsoft's mainline Serbian (`sr-RS`, both scripts) is ekavian; `sr-Cyrl-BA` (Bosnia)
  leans ijekavian. Ekavian is the default for "Serbian" almost everywhere.
- **Recommendation: target ekavian for `sr`.** It's the standard digital Serbian for the Serbia audience and pairs
  naturally with either script. `sr-ije` (ijekavian) exists if a Montenegro/Bosnia variant is ever wanted, but don't
  ship it as the base. `high`.

### Formality / T-V (ti vs Vi)

- Covered in Formality above: use the informal singular **`ti`** register (consumer-brand decision; bare imperatives
  on buttons). `high`. (Listed here too so it's not relitigated as a "decision point".) See
  [`formal-informal-decisions.md`](formal-informal-decisions.md).

### Gender agreement (the gendered-grammar trap)

- **The fact.** Serbian inflects for gender in the past tense, adjectives, and participles, so a sentence that describes
  what the user did or addresses the user can leak a gender the UI can't know: "обрисали сте" assumes a form, and
  adjectives like "сигуран"/"сигурна" (sure, masc./fem.) differ.
- **Strategy (from Microsoft's Serbian guide, verified against the reference pile, 2026-06-20):** avoid generic gendered
  pronouns and rewrite for neutrality:
  - Use the impersonal "se" construction or 2nd-person-plural where it dodges a gendered past participle ("Датотека је
    обрисана." = the file was deleted, passive/impersonal, rather than "Обрисали сте..." which can read gendered).
  - Use a role/noun ("корисник", "клијент") or "особа"/"појединац" instead of a he/she pronoun.
  - Prefer a determiner over a possessive pronoun ("овај документ" not "његов документ").
- **Recommendation.** Phrase UI strings to describe the object/state ("Датотека је премештена.") rather than the user's
  gendered action wherever a past participle would otherwise agree with the user. Where the user must be addressed,
  imperatives (singular `ti` form) are gender-neutral, so they dodge the participle problem. `high`.

### Plurals (the Slavic plural-by-number trap)

- See the Plurals section. CLDR `sr` is **one, few, other** (no "many"); the count-to-category mapping is by the last
  digit, not by magnitude, so don't pattern-match off English singular/plural. `high`.

## Terminology and glossary

Serbian IT terminology here is triangulated from GNOME Nautilus (the file-manager domain, both scripts), Xfce Thunar
(Cyrillic), and Microsoft terminology (both scripts). There's no macOS Serbian to anchor on (Apple ships no Serbian), so
GNOME's file-manager catalog is the strongest source for this domain. Values are shown Cyrillic first, then the 1:1
Latin transliteration. Format per term: `chosen · sources · confidence`. Evidence verified against the reference pile on
2026-06-20; sources decide the term, Cmdr writes its own value (Apple/MS copyrighted, GNOME/Xfce GPL, never pasted).

- **folder** → фасцикла · fascikla · GNOME and MS both use "фасцикла"/"fascikla". Thunar's "омот" (omot) is dated; don't use it. `high`
- **file** → датотека · datoteka · GNOME uses "датотека"/"datoteka" in both scripts; prefer it. MS terminology is inconsistent (Cyrillic db has the colloquial "фајл", Latin db has "datoteka"); for a file manager, "датотека"/"datoteka" reads cleaner and more native. `high`
- **trash** → смеће · smeće · GNOME, Thunar, MS all agree. "Move to trash" = "Премести у смеће" / "Premesti u smeće". `high`
- **move (to trash)** → преместити · premestiti · GNOME "Премести у смеће", MS "преместити"/"premestiti". `high`
- **rename** → преименовати · preimenovati · GNOME "Преименуј"/"Preimenuj", MS "Preimenuj". Button form is the bare imperative. `high`
- **copy** → копирати · kopirati · MS "копирати"/"kopirati"; Thunar also uses "умножити" (umnožiti), an acceptable native synonym. Prefer "копирати"/"kopirati". `high`
- **delete** → избрисати · izbrisati · MS "Избриши"/"Izbriši" for the destructive delete. Reserve for permanent delete; the safe path is "Премести у смеће". `high`
- **search** → претрага (noun) · pretraga / тражити (verb) · tražiti · GNOME "Претрага"/"Pretraga" (noun), MS "тражити"/"tražiti" (verb). `high`
- **cancel** → отказати · otkazati · GNOME/MS "Откажи"/"Otkaži" on buttons. `high`
- **folder (new)** → нова фасцикла · nova fascikla · GNOME "Нова фасцикла"/"Nova fascikla". `high`
- **bookmark** → обележивач · obeleživač · GNOME "обележивач"/"obeleživač". `high`
- **pane** → окно · okno · The window region holding a file list; "окно"/"okno" is the standard Serbian IT term for a UI pane. `tentative` (no direct file-manager source string; standard IT usage)
- **tab** → картица · kartica · Standard Serbian for a UI tab. `tentative`
- **volume** → волумен · volumen / диск · disk · A mounted disk volume. "волумен"/"volumen" is the technical term; MS notes "диск"/"disk" reads more informal/friendly. Prefer "диск"/"disk" for the user-facing label, "волумен"/"volumen" where the precise storage sense matters. `tentative`
- **viewer** → приказивач · prikazivač · The file-preview surface. `tentative` (no source string; "preview" sense).

Add terms as they come up in this same `chosen · sources · confidence` shape, showing both scripts; keep the catalog
consistent and transliterate values 1:1 if the shipped script flips.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Since macOS ships no Serbian, the macOS UI names Cmdr opens into (System
Settings panes, etc.) appear in whatever language the user's Mac runs in (commonly English); don't invent Serbian
translations for them, refer to them as the user's own Mac shows them.

## Plurals

CLDR categories: `one`, `few`, `other` (verified with `new Intl.PluralRules('sr')`; same for `sr-Latn`). Write all three
branches. There is **no `many`** category for Serbian, don't add one.

- **The mapping is by last digit, not magnitude** (the Slavic trap): `one` = ends in 1 but not 11 (1, 21, 31, 101);
  `few` = ends in 2/3/4 but not 12/13/14 (2, 3, 4, 22, 23); `other` = everything else including 5-20, 11-14, 0, and
  decimals. So "21 датотека" (one), "22 датотеке" (few), "5 датотека" (other). Never assume "1 = singular, 2+ = plural".
- **Case agreement varies per branch.** Serbian counted nouns take different case forms: `one` takes nominative singular
  ("1 датотека"), `few` takes genitive singular / paucal ("2 датотеке"), `other` takes genitive plural ("5 датотека").
  Write the correct noun form inside each branch, don't reuse one stem.
- GNOME's gettext catalog uses a 4-form rule that splits the digit forms differently from CLDR; ignore it. Cmdr's plural
  selection runs on `Intl.PluralRules`, so author to the CLDR `one`/`few`/`other` set above. The `desktop-i18n-plural`
  check requires every plural message to cover these three for `sr`.

## Notes and decisions

- **Quotation marks: `„…“`** (low-opening, high-closing) is the standard Serbian form, in both scripts. The reference
  pile uses exactly this (`„%s“`) (verified against the reference pile, 2026-06-20). Avoid English `"…"`.
- **Numbers and dates come from the formatter layer.** Serbian uses a comma decimal and a dot/space thousands separator;
  `formatNumber()`/`formatBytes()` produce these from the locale. Never hardcode separators in a string.
- **Transliteration is mechanical but not blind.** When flipping a value between scripts, the Latin digraphs lj, nj, dž
  map to single Cyrillic letters (љ, њ, џ); a naive letter-by-letter swap breaks on those. If both scripts ship, generate
  one from the other with a real Serbian transliterator and spot-check the digraphs, don't hand-swap letters.
- **Length.** Serbian runs close to English in width (a touch longer for some compounds); overflow risk is moderate.
  Overflow-check the layout against the pseudolocale (`en-XA`). Cyrillic and Latin render at similar widths.
- **ICU mechanics** (catalog-level, not Serbian-specific): double every apostrophe in a value (`'` becomes `''`), and
  keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
