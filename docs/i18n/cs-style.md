# Czech (cs) translation style guide

Working notes for translating Cmdr into Czech. Read [`README.md`](README.md) for how this fits the translation process,
and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into Czech.

Well-sourced: the pile has macOS Finder/AppKit (highest authority), MS terminology, MS style guide, GNOME Nautilus, and
Xfce Thunar (`_ignored/i18n/cs/`). Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. Only the first is a true open flag; the rest carry a confident default and are
listed so they're never relitigated.

- **Address form: neutral second-person plural (vykání-shaped, gender-neutral) recommended, needs a sign-off (high).**
  Czech distinguishes formal `vy` (vykání) from informal `ty` (tykání). MS Czech is explicit: "use the neutral 'you'
  (second-person plural) whenever possible; don't use the informal 'you' (tykání) unless appropriate (a Skype chat
  between friends)" (verified 2026-06-20). The neutral plural is what software uses, and its past-tense participle is
  gender-neutral, which solves the gender problem too (see decision point). Recommended default: **neutral
  second-person plural throughout.** Flagging because Cmdr's English voice is warm-and-informal, so David may want to
  confirm the register shift is intended.
- **`volume` term (tentative).** No clean macOS "volume" string in the Czech pile; candidates are `svazek` (literal
  volume) or `oddíl` (partition/section). See the glossary; worth a native check.

## Voice and tone

Friendly, concise, active, calm, but **neutral-formal in address** (vykání-shaped plural). The warmth comes from clear,
short, helpful phrasing, not from informal `ty`. MS Czech says the Microsoft voice "avoids an unnecessarily formal tone"
and to "look for more informal or colloquial wording" while still using the neutral plural (verified 2026-06-20) - so
keep sentences light and everyday, just not tykání. Error messages stay calm and actionable: phrase the problem and the
next step, and don't use "chyba" (error) or "selhalo" (failed) as a bare status label the way English avoids
"error"/"failed".

## Formality

- **Neutral second-person plural, throughout. Never tykání (informal `ty`).** The formal uppercase `Vy`/`Vás`
  (vykání proper) belongs to personal correspondence (emails, letters to a specific named user), not product UI; MS
  Czech reserves it for exactly that (verified 2026-06-20). In Cmdr UI, use the lowercase neutral plural.
- **Action labels (buttons, menu items): infinitive, not imperative.** This is the Slavic UI norm and what macOS Czech
  shows: "Kopírovat" (Copy), "Uložit" (Save), "Smazat" (Delete), "Otevřít" (Open), "Zrušit" (Cancel), "Odpojit"
  (Disconnect) (macOS AppKit, verified 2026-06-20). Avoid bare imperatives like "Kopíruj"/"Ulož": they bark a command
  and read as tykání-flavored.
- **Full sentences addressed to the user: neutral vy-plural.** "Opravdu chcete odstranit tyto soubory?" (Are you sure
  you want to delete these files?). So the rule is dual: **standalone labels = infinitive; sentences to the user =
  neutral vy-plural.** Confidence: high (macOS and MS agree).

## Decision points

- **Script: Latin, no decision.** Czech is written in the Latin alphabet with diacritics (á, č, ď, é, ě, í, ň, ó, ř,
  š, ť, ú, ů, ý, ž). No script choice. Confidence: high.
- **Regional variant: one, `cs` (`cs-CZ`).** Czech is standardized only in Czechia; no second national standard, no
  pt-BR/pt-PT-style split. Don't build a variant matrix. (Slovak is a separate language, `sk`, not a variant.)
  Confidence: high.
- **Gender / inclusive language: the neutral vy-plural already solves most of it (high on the problem, high on the
  fix).** Czech past tense uses gendered l-participles (-l masc, -la fem). A singular-addressed "you deleted" forces a
  gender guess, but the neutral second-person plural participle is `-li`, which is **gender-neutral**: "Smazali jste 3
  soubory" works for any user. This is a second reason the neutral plural is the right call. Where a singular
  adjective/participle would still agree with the user's gender, **rewrite impersonally**: "Kopírování dokončeno"
  (Copying complete) or "Soubor byl odstraněn" (The file was deleted) rather than "Odstranili jste…". Recommendation:
  lean on the gender-neutral vy-plural for user actions, and impersonal/nominal phrasing for system-state messages.
  Confidence: high.
- **Capitalization: sentence case everywhere (high).** Czech capitalizes only the first word and proper nouns in
  titles, menu items, labels, and buttons. English title case is wrong ("Zobrazit skryté soubory", not "Zobrazit Skryté
  Soubory"). Matches Cmdr's sentence-case rule with no friction.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/cs/` (macOS Finder/AppKit, MS
terminology, GNOME Nautilus, Xfce Thunar) on 2026-06-20; macOS strings cited are what Czech Finder/AppKit actually show.
Sources decide the term; Cmdr writes its own value (Apple/MS copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (sources agree):

- **folder: `složka`** · macOS Finder ("Složka"), GNOME ("Složka"). Plural "složky". `high`.
- **file: `soubor`** · macOS, GNOME ("Soubor"). Plural "soubory" (few), "souborů" (other/genitive). `high`.
- **directory: `adresář`** · MS terminology; use only where the technical filesystem sense matters, else "složka".
  `high`.
- **trash: `koš`** · macOS Finder maps both "Trash" and "Bin" to "Koš". `high`.
- **move to trash: `přesunout do koše`** · aligns with macOS "Koš"; GNOME phrasing. `high`.
- **delete (permanent): `smazat`** · macOS AppKit ("Smazat"). Reserve for destructive delete; use "přesunout do koše"
  for the safe move. `high`.
- **eject: `vysunout`** · macOS Finder. Infinitive label "Vysunout". `high`.
- **copy: `kopírovat`** · macOS AppKit ("Kopírovat"). `high`.
- **cancel: `zrušit`** · macOS AppKit ("Zrušit"). `high`.
- **open: `otevřít`** · macOS AppKit ("Otevřít"). `high`.
- **save: `uložit`** · macOS AppKit ("Uložit"). `high`.
- **disconnect: `odpojit`** · macOS AppKit ("Odpojit"). `high`.
- **search: `hledat` (verb) / `hledání` (noun)** · macOS Finder ("Hledat ve Finderu"). `high`.
- **network: `síť`** · macOS Finder ("Síť"). `high`.
- **shared: `sdíleno`** · macOS Finder ("Sdíleno"). `high`.

Tentative / needs a native check:

- **volume: `svazek`** · no clean macOS reference; `svazek` is the literal "volume", `oddíl` is "partition/section".
  Default to `svazek` for a mounted disk. `tentative`.
- **tab (UI tab): `karta`** · MS/GNOME convention; the macOS "Tab" string is the keyboard Tab key (Tabulátor), wrong
  sense. Use `karta` for the pane tab. `tentative`.
- **pane: `panel`** · GNOME uses "panel" for window regions; the two file lists are "panely". `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. macOS UI names Cmdr opens into should match what a Czech macOS shows ("Koš",
"Nastavení").

## Plurals

CLDR categories for `cs`: `one`, `few`, `many`, `other` (verified with `new Intl.PluralRules('cs')`). Write all four.

- **one**: integer 1 only (`i=1, v=0`). "1 soubor".
- **few**: integers 2-4 (`i=2..4, v=0`). "2 soubory".
- **many**: any number with a decimal fraction (`v≠0`). "1,5 souboru". This is the **decimal/fraction** bucket, not the
  large-number bucket.
- **other**: everything else, including 0 and 5+ (`5 souborů`, `0 souborů`, `100 souborů`).
- **Trap: `many` is the decimal form, not "lots".** Translators from a Polish/Russian background (where "many" is the
  big-number bucket) get this backwards. In Czech, 5+ integers go to `other`; `many` only fires on decimals. (Same trap
  as Slovak.)
- Forms map to cases: 1 = nominative sg, 2-4 = nominative pl, 5+/0 = genitive pl, decimals = genitive sg. Keep
  article/adjective agreement inside each branch. The `desktop-i18n-plural` check requires all four.

## Notes and decisions

- **Quotation marks: `„…"`** (low-9 opening U+201E, high-6 closing U+201C), the standard Czech form (same shape as
  German/Slovak). Avoid straight ASCII `"` and English `"…"`.
- **Numbers and dates come from the formatter layer.** Czech uses a comma decimal and space thousands separator
  (1 000); `formatNumber()`/`formatBytes()` produce these from the locale. Never hardcode separators in a string.
- **Length.** Czech runs somewhat longer than English (case endings, longer compounds), so overflow-check the layout
  against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
