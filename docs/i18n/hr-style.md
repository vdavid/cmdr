# Croatian (hr) translation style guide

Working notes for translating Cmdr into Croatian (hrvatski). Read [`README.md`](README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes
carry into Croatian.

Well-sourced: the pile has macOS Finder/AppKit (highest authority), MS terminology, MS style guide, GNOME Nautilus, and
Xfce Thunar (`_ignored/i18n/hr/`). Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. Only the first is a true open flag; the rest carry a confident default and are
listed so they're never relitigated.

- **Address form: formal second-person plural (`vi`) recommended, needs a sign-off (high).** Croatian distinguishes
  formal `vi` (persiranje) from informal `ti`. MS Croatian addresses the user with the polite second person `vi` and
  notes the Microsoft voice "avoids an unnecessarily formal tone" while staying respectful (verified 2026-06-20).
  Standard software register is `vi`. Recommended default: **`vi` throughout.** Flagging because Cmdr's English voice is
  warm-and-informal, so David may want to confirm the register shift is intended.
- **`volume` term (tentative).** No clean macOS "volume" string in the Croatian pile; candidates are `pogon` (drive,
  the MS-preferred everyday word) or a more literal partition term. See the glossary; worth a native check.

## Voice and tone

Friendly, concise, active, calm, but **formal in address** (`vi`). MS Croatian explicitly steers away from
overly-formal, heavy literary phrasing ("don't follow the English informal style strictly, but avoid heavy
literary/journalistic register"), and prefers everyday words over professional ones (`disk` over `pogon` where "drive"
means "disk"; `računalo` over `osobno računalo`) (verified 2026-06-20). So the warmth comes from short, plain,
helpful sentences, not from informal `ti`. Error messages stay calm and actionable: name the problem and the next step,
and don't use "greška" (error) or "neuspjelo" (failed) as a bare status label the way English avoids "error"/"failed".

## Formality

- **Formal `vi` for full sentences addressed to the user.** "Jeste li sigurni da želite izbrisati ove datoteke?" (Are
  you sure you want to delete these files?). Never informal `ti`. Confidence: high (MS Croatian).
- **Action labels (buttons, menu items): bare imperative, second-person singular form.** This is what macOS Croatian
  shows: "Kopiraj" (Copy), "Izreži" (Cut), "Zalijepi" (Paste), "Spremi" (Save), "Obriši" (Delete), "Otvori" (Open),
  "Odustani" (Cancel) (macOS AppKit, verified 2026-06-20). So the rule is dual: **standalone action labels =
  imperative; full sentences to the user = `vi`.** This matches the Slavic/Apple UI norm and is not a contradiction:
  the imperative label is an action name, not address. Confidence: high.

## Decision points

- **Script: Latin, no decision.** Croatian is written in the Latin alphabet with diacritics (č, ć, đ, š, ž, plus the
  digraphs dž, lj, nj). No script choice. Confidence: high.
- **Regional variant: one, `hr` (`hr-HR`).** Croatian is standardized only in Croatia; no second national standard.
  (Bosnian `bs` and Serbian `sr` are separate languages in this pile, not variants of Croatian.) Don't build a variant
  matrix. Confidence: high.
- **Gender / inclusive language (high on the problem, high on the fix).** Croatian past tense uses gendered
  l-participles (-o masc, -la fem), so a singular-addressed "you deleted" forces a gender guess. The formal `vi`-plural
  participle is `-li`, which reads as the default/masculine plural but is the conventional neutral software form;
  where it would still expose gender, **rewrite impersonally**: "Kopiranje dovršeno" (Copying complete) or "Datoteka je
  izbrisana" (The file was deleted) rather than "Izbrisali ste…". Recommendation: lean on `vi`-plural for user actions,
  impersonal/nominal phrasing for system-state messages. Confidence: high.
- **Capitalization: sentence case everywhere (high).** Croatian capitalizes only the first word and proper nouns in
  titles, menu items, labels, and buttons. English title case is wrong ("Prikaži skrivene datoteke", not "Prikaži
  Skrivene Datoteke"). Matches Cmdr's sentence-case rule with no friction.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/hr/` (macOS Finder/AppKit, MS
terminology, GNOME Nautilus, Xfce Thunar) on 2026-06-20; macOS strings cited are what Croatian Finder/AppKit actually
show. Sources decide the term; Cmdr writes its own value (Apple/MS copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (sources agree):

- **file: `datoteka`** · macOS, GNOME. Plural "datoteke". `high`.
- **folder: `mapa`** · macOS Finder, MS terminology. Plural "mape". `high`.
- **trash: `smeće`** · macOS Finder ("Smeće"). `high`.
- **move to trash: `premjesti u smeće`** · macOS Finder ("Premjesti … u Smeće"). `high`.
- **delete (permanent): `obriši`** · macOS AppKit ("Obriši"). Reserve for destructive delete; use "premjesti u smeće"
  for the safe move. `high`.
- **copy: `kopiraj`** · macOS AppKit ("Kopiraj"). `high`.
- **cut: `izreži`** · macOS AppKit ("Izreži"). `high`.
- **paste: `zalijepi`** · macOS AppKit ("Zalijepi"). `high`.
- **cancel: `odustani`** · macOS AppKit ("Odustani"). `high`.
- **open: `otvori`** · macOS AppKit ("Otvori"). `high`.
- **save: `spremi`** · macOS AppKit ("Spremi"). `high`.
- **network: `mreža`** · macOS Finder ("Mreža"). `high`.
- **eject: `izbaci`** · macOS Finder convention; confirm against the Croatian eject string. `high`.

Tentative / needs a native check:

- **volume: `pogon`** · MS prefers `pogon` for "drive"; no clean macOS "volume" string. Default to `pogon` for a
  mounted disk, or a literal partition term where the technical sense matters. `tentative`.
- **pane: `okno`** · the two file lists are "okna"; GNOME/window-region convention. `tentative`.
- **tab (UI tab): `kartica`** · MS/GNOME convention; the macOS "Tab" string is the keyboard key, wrong sense. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. macOS UI names Cmdr opens into should match what a Croatian macOS shows
("Smeće", "Postavke").

## Plurals

CLDR categories for `hr`: `one`, `few`, `other` (verified with `new Intl.PluralRules('hr')`, 2026-06-20). Write all
three. This is the Slavic one/few/other pattern (same as bs, sr).

- **one**: numbers ending in 1 but not 11 (1, 21, 31, …). "1 datoteka", "21 datoteka".
- **few**: numbers ending in 2-4 but not 12-14 (2, 3, 4, 22, 23, …). "2 datoteke", "3 datoteke".
- **other**: everything else, including 0, 5-20, and the teens (5, 11, 12, 100). "5 datoteka", "0 datoteka".
- Forms map to case: `one` = nominative sg, `few` = genitive sg/paucal (datoteke), `other` = genitive pl (datoteka).
  Keep article/adjective agreement inside each branch. The `desktop-i18n-plural` check requires all three.
- **Trap:** unlike Czech/Slovak, Croatian's CLDR set has no `many` (decimals fall into `other`/`few` by rule), so don't
  copy a four-category Slavic structure here.

## Notes and decisions

- **Quotation marks: `„…”`** (low-9 opening U+201E, high-9 closing U+201D) is the standard Croatian form; the guillemet
  form `»…«` also appears in print. Avoid straight ASCII `"` and English `"…"`.
- **Numbers and dates come from the formatter layer.** Croatian uses a comma decimal and a dot/space thousands
  separator; `formatNumber()`/`formatBytes()` produce these from the locale. Never hardcode separators in a string.
- **Length.** Croatian runs somewhat longer than English (case endings), so overflow-check the layout against the
  pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
