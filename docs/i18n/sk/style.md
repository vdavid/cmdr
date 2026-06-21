# Slovak (sk) translation style guide

Working notes for translating Cmdr into Slovak. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Slovak.

## Decisions to confirm with David

These are the calls a translator can't make alone. The rest of this guide assumes them; only the first is a true open
flag, the rest carry a confident default and are listed so they're never relitigated.

- **Address form: `vykanie` (formal) recommended, needs a sign-off (high).** Slovak distinguishes formal `vy` (vykanie)
  from informal `ty` (tykanie). The Slovak software-localization norm and Microsoft/Apple/Mozilla-family products all
  use vykanie; informal `ty` reads as presumptuous to a Slovak adult even in a "friendly" app (Slovak friendliness is
  not tykanie). Recommended default: **vykanie throughout.** Flagging because it sets the tone for every sentence the
  app speaks, and because Cmdr's English voice is deliberately warm-and-informal, so David may want to confirm the
  register shift is intended.
- **`volume` = `oddiel` vs `zväzok` (tentative).** macOS Slovak Finder uses `oddiel` (literally "partition/section") for
  server volumes ("Serverové oddiely"). It's the macOS-backed term, so it's the default, but `oddiel` also means
  "partition", which can read oddly for a whole mounted disk. See the glossary; worth a native check.

## Voice and tone

Friendly, concise, active, calm, but **formal in address** (vykanie). The warmth comes from clear, short, helpful
phrasing, not from informal `ty`. Error messages stay calm and actionable: phrase the problem and the next step, and
don't use "chyba" (error) or "zlyhalo" (failed) as a status label the way English avoids "error"/"failed" (for example
"Súbor sa nepodarilo premenovať. Skúsiť znova?" rather than a bare error code). When the app speaks a full sentence to
the user, use the polite vy-plural ("Naozaj chcete odstrániť tieto súbory?").

## Formality

- **`vykanie` (formal `vy`), throughout.** Never tykanie. Don't capitalize `Vy`/`Vás`/`Vám` mid-sentence: capitalized
  formal pronouns belong to personal correspondence, not product UI.
- **Action labels (buttons, menu items): infinitive, not imperative.** This is the Slavic UI norm and what macOS Slovak
  shows: "Kopírovať" (Copy), "Uložiť" (Save), "Vymazať" (Delete), "Premenovať" (Rename), "Otvoriť" (Open). Avoid bare
  imperatives like "Kopíruj"/"Ulož": they bark a command and read as tykanie-flavored.
- **Full sentences addressed to the user: polite vy-plural.** "Zadajte heslo" (Enter the password), "Naozaj chcete
  odstrániť…". So the rule is dual: **standalone labels = infinitive; sentences to the user = polite vy-form.**
  (verified against the reference pile, 2026-06-20: macOS Finder shows "Pripojiť k serveru", "Vysunúť", "Vytvoriť
  priečinok", all infinitive.)

## Decision points

- **Script: Latin, no decision.** Slovak is written in the Latin alphabet with diacritics (á, ä, č, ď, é, í, ĺ, ľ, ň, ó,
  ô, ŕ, š, ť, ú, ý, ž). No script choice to make. Confidence: high.
- **Regional variant: one, `sk` (`sk-SK`).** Slovak is standardized only in Slovakia; there's no second national
  standard (no pt-BR/pt-PT-style split). Don't build a variant matrix. Confidence: high.
- **Gender / inclusive language: the vy-plural already solves most of it (high on the problem, tentative on the single
  best fix).** Slovak past tense uses gendered l-participles (-l masc, -la fem). A singular-addressed "you deleted"
  forces a gender guess, but the formal vy-plural participle is `-li`, which is **gender-neutral**: "Vymazali ste 3
  súbory" works for any user. This is a second reason vykanie is the right call. Where a singular adjective/participle
  would still agree with the user's gender, **rewrite impersonally**: "Kopírovanie dokončené" (Copying complete) or
  "Súbor bol odstránený" (The file was deleted) rather than "Odstránili ste…". Impersonal/nominal phrasing sidesteps
  gender entirely and reads tighter. Recommended default: lean on the gender-neutral vy-plural for user actions, and
  impersonal/nominal phrasing for system-state messages.
- **Capitalization: sentence case everywhere (high).** Slovak capitalizes only the first word and proper nouns in
  titles, menu items, labels, and buttons. English title case is wrong ("Zobraziť skryté súbory", not "Zobraziť Skryté
  Súbory"). This matches Cmdr's existing sentence-case rule with no friction.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Confidence is `confirmed` (a native human signed off), `high`
(authoritative sources agree), or `tentative` (sources conflict or none had it). Evidence verified against the reference
pile (`_ignored/i18n/sk/`: macOS Finder/AppKit, MS terminology, GNOME Nautilus, Xfce Thunar) on 2026-06-20; macOS
strings cited are what Slovak Finder/AppKit actually show. Sources decide the term; Cmdr writes its own value (Apple/MS
copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (sources agree):

- **folder: `priečinok`** · macOS Finder ("Priečinok", "Vytvoriť priečinok", "Zdieľaný priečinok"), GNOME. Definite
  forms inflect; plural "priečinky". `high`.
- **file: `súbor`** · macOS Finder/AppKit ("Súbor"), GNOME. Plural "súbory" (few), "súborov" (other). `high`.
- **directory: `adresár`** · MS terminology; use only where the technical filesystem sense matters, else "priečinok".
  `high`.
- **trash: `kôš`** · macOS Finder maps both "Trash" and "Bin" to "Kôš"; GNOME "kôš". `high`.
- **move to trash: `presunúť do koša`** · GNOME ("Presunúť do koša"), aligns with macOS "Kôš". `high`.
- **delete (permanent): `vymazať`** · macOS AppKit ("Vymazať"). Reserve for the destructive delete; use "presunúť do
  koša" for the safe move. `high`.
- **eject: `vysunúť`** · macOS Finder ("Vysunúť"), GNOME ("Vysunie"). Infinitive label "Vysunúť". `high`.
- **copy: `kopírovať`** · macOS AppKit ("Kopírovať"). `high`.
- **cancel: `zrušiť`** · macOS AppKit ("Zrušiť"). Imperative-infinitive on buttons. `high`.
- **open: `otvoriť`** · macOS AppKit ("Otvoriť"). `high`.
- **search: `vyhľadať` (verb) / `vyhľadávanie` (noun)** · macOS Finder shows both. `high`.
- **server: `server`** · macOS Finder ("Pripojiť k serveru", "Serverové oddiely"). Connect-to-server verb is "Pripojiť".
  `high`.
- **disconnect: `odpojiť`** · macOS AppKit ("Odpojiť"). `high`.
- **sidebar: `postranný panel`** · GNOME ("Postranný panel"). `high`.
- **bookmark: `záložka`** · GNOME ("…do záložiek"). Plural "záložky". `high`.
- **sort: `zoradiť`** · GNOME ("Zoradiť"). `high`.

Tentative / needs a native check:

- **volume: `oddiel`** · macOS Finder ("Serverové oddiely" = server volumes). macOS-backed, so the default, but `oddiel`
  literally means "partition/section"; `zväzok` is the more literal "volume". Flagged above. `tentative`.
- **tab (UI tab): `karta`** · macOS AppKit maps "Tab" to "Tabulátor", but that's the keyboard Tab key, not the UI tab.
  Slovak UI tabs are "karty" (MS/GNOME convention). Use **`karta`** for the pane tab; never "tabulátor" (that's the
  key). `tentative` (macOS string is the wrong sense, so it can't back this directly).
- **pane: `panel`** · no direct macOS "pane" term; GNOME uses "panel" for window regions ("Postranný panel"). The two
  file lists are "panely". `tentative`.
- **listing: `zoznam súborov`** · GNOME renders "List View" as "Zobrazenie zoznamu"; "zoznam súborov" reads natural for
  the file list. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. macOS UI names Cmdr opens into should match what a Slovak macOS shows
("Kôš", "Nastavenia").

## Plurals

CLDR categories: `one`, `few`, `many`, `other` (verified with `new Intl.PluralRules('sk')`). Write all four.

- **one**: integer 1 only (`i=1, v=0`). "1 súbor".
- **few**: integers 2–4 (`i=2..4, v=0`). "2 súbory".
- **many**: any number with a decimal fraction (`v≠0`). "1,5 súboru". This is the **decimal/fraction** bucket, not the
  large-number bucket.
- **other**: everything else, including 0 and 5+ (`5 súborov`, `0 súborov`, `100 súborov`).
- **Trap: `many` is the decimal form, not "lots".** Translators from a Polish/Russian background (where "many" is the
  big-number bucket) get this backwards. In Slovak, 5+ integers go to `other`; `many` only fires on decimals.
- Forms map to cases: 1 = nominative sg (súbor), 2–4 = nominative pl (súbory), 5+/0 = genitive pl (súborov), decimals =
  genitive sg (súboru). Keep the article/adjective agreeing with the counted noun inside each branch. The
  `desktop-i18n-plural` check requires every plural message to cover all four.

## Notes and decisions

- **Quotation marks: `„…"`** (low-9 opening U+201E, high-6 closing U+201C), the standard Slovak form, same shape as
  German/Czech. Nested/secondary: **`»…«`** (guillemets pointing inward, not the French `«…»`). Avoid straight ASCII `"`
  and English `"…"`.
- **Numbers and dates come from the formatter layer.** Slovak uses a comma decimal and space thousands separator (1
  000); `formatNumber()`/`formatBytes()` produce these from the locale. Never hardcode separators in a string.
- **Length.** Slovak runs somewhat longer than English (case endings, longer compounds), so overflow-check the layout
  against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/sk/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
