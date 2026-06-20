# Polish (pl) translation style guide

Working notes for translating Cmdr into Polish. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Polish.

Polish is a major, well-resourced language: the pile has all five sources (`_ignored/i18n/pl/`: macOS Finder/AppKit, MS
terminology + style guide, GNOME Nautilus, Xfce Thunar), so most terms reach `high`. Evidence verified against the pile
on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. The rest of the guide assumes them; only the first is a true open flag, the
rest carry a confident default and are listed so they're never relitigated.

- **Address form: RESOLVED to informal `Ty`** (consumer-brand evidence; see Formality and
  [`formal-informal-decisions.md`](formal-informal-decisions.md)). No longer open. Polish still prefers depersonalized
  phrasing where natural (it also dodges the gendered-past-tense trap), but direct address is informal `Ty`, never
  `Pan/Pani`.
- **`folder` = `folder` vs `katalog` (high, but worth a glance).** macOS Finder shows "Folder" verbatim (the English
  loanword, fully naturalized in Polish); GNOME uses "Katalog". For a macOS app, match Finder: **`folder`**. `katalog`
  reads more like the technical "directory". See glossary.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. The Polish Microsoft voice is
explicitly "warm and relaxed, less formal" (MS style guide, verified 2026-06-20), which aligns with Cmdr. The warmth
comes from short, clear, helpful phrasing, not from grammatically informal address. Error messages stay calm and
actionable: phrase the problem and the next step, and don't use "błąd" (error) or "nie powiodło się" (failed) as a bare
status label the way English avoids "error"/"failed".

## Formality

**Verdict: informal `Ty`, not `Pan/Pani`.** Consumer brands (IKEA, Spotify, Netflix, and peers) address Polish users
with informal `Ty`, which fits Cmdr's friendly personal voice. Formality decision recorded in
[`formal-informal-decisions.md`](formal-informal-decisions.md). Polish still leans heavily on depersonalized phrasing
where it reads naturally (it also sidesteps the gendered-past-tense trap), but the register, wherever the user is
addressed, is informal `Ty`, never the formal `Pan/Pani`.

- **Action labels (buttons, menu items): use the imperative or, where it reads as a feature name, a verbal noun.**
  macOS Polish shows imperative forms: "Kopiuj" (Copy), "Wklej" (Paste), "Wytnij" (Cut), "Otwórz" (Open), "Usuń"
  (Delete), "Anuluj" (Cancel), "Przenieś" (Move) (macOS AppKit, verified 2026-06-20). These bare imperatives are the
  standard OS convention and sit fine under a `Ty` register. GNOME prefers verbal nouns for some entries ("Zmiana
  nazwy" = renaming), but macOS imperative is the file-manager norm to match.
- **System messages: impersonal/passive where natural.** "Nie można utworzyć folderu" (Cannot create the folder),
  "Usunięto 3 pliki" (3 files deleted), "Kopiowanie zakończone" (Copying complete). Impersonal phrasing dodges the
  gendered-past-tense trap, so prefer it for system-state messages.
- **Where a full sentence addresses the user, use `Ty`.** "Czy chcesz usunąć te pliki?" (Do you want to delete these
  files?) is on-brand; the impersonal "Czy na pewno usunąć te pliki?" is also fine and gender-safe. Never `Pan/Pani`.

So the rule: **labels = imperative; system messages = impersonal/passive where it reads natural; direct address = `Ty`,
never `Pan/Pani`.** This keeps the gender-safe phrasing while landing the warm consumer-brand register.

## Decision points

- **Script: Latin, no decision.** Polish uses the Latin alphabet with diacritics (ą, ć, ę, ł, ń, ó, ś, ź, ż). No script
  choice. Confidence: high.
- **Regional variant: one, `pl` (`pl-PL`).** Polish is standardized only in Poland; no second national standard, no
  pt-BR/pt-PT-style split. Don't build a variant matrix. Confidence: high.
- **Gender / inclusive language: solved by impersonal phrasing (high).** Polish past tense and adjectives agree with
  gender (-ł masc / -ła fem; "pewien"/"pewna"). Any sentence that makes the user the subject ("you deleted",
  "are you sure") forces a gender guess. The impersonal/passive style above sidesteps this entirely: "Usunięto 3 pliki"
  has no gendered subject, "Kopiowanie zakończone" is a verbal noun. Recommended default: depersonalize system-state
  messages; never emit a user-gendered participle. This is a second, independent reason the impersonal style is the
  right call (not only register but gender safety). Confidence: high.
- **Capitalization: sentence case everywhere (high).** Polish capitalizes only the first word and proper nouns in
  titles, menu items, labels, and buttons. English title case is wrong ("Pokaż ukryte pliki", not "Pokaż Ukryte
  Pliki"). Matches Cmdr's existing sentence-case rule. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Confidence: `confirmed` (native sign-off), `high` (authoritative
sources agree), `tentative` (sources conflict or none had it). Evidence from `_ignored/i18n/pl/` (macOS Finder/AppKit,
MS terminology, GNOME Nautilus, Xfce Thunar), verified 2026-06-20. Sources decide the term; Cmdr writes its own value
(Apple/MS copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (sources agree):

- **folder: `folder`** · macOS Finder ("Folder"). GNOME uses "Katalog" (= directory); for a macOS app match Finder.
  Plural "foldery", genitive pl "folderów". `high`.
- **file: `plik`** · GNOME ("Plik"), universal. Plural "pliki" (few), "plików" (many/other). `high`.
- **directory: `katalog`** · GNOME, MS. Use only where the technical filesystem sense matters; else "folder". `high`.
- **trash: `kosz`** · macOS Finder maps both "Trash" and "Bin" to "Kosz"; GNOME "Kosz". `high`.
- **move to trash: `przenieś do kosza`** · GNOME ("Przeniesienie do kosza" as a noun; imperative "Przenieś do kosza").
  `high`.
- **delete (permanent): `usuń`** · macOS AppKit ("Usuń"). `high`.
- **copy: `kopiuj`** · macOS AppKit ("Kopiuj"). Imperative on buttons. `high`.
- **paste: `wklej`** · macOS AppKit ("Wklej"). `high`.
- **cut: `wytnij`** · macOS AppKit ("Wytnij"). `high`.
- **cancel: `anuluj`** · macOS AppKit/Finder ("Anuluj"). `high`.
- **open: `otwórz`** · macOS AppKit ("Otwórz"). `high`.
- **save: `zachowaj`** · macOS AppKit ("Zachowaj"). Note: macOS uses "Zachowaj"; MS/general software often "Zapisz".
  Match macOS for a macOS app. `high`.
- **move: `przenieś`** · macOS AppKit ("Przenieś"). `high`.
- **search: `szukaj` (verb, imperative) / `wyszukiwanie` (noun)** · macOS AppKit ("Szukaj"). `high`.
- **eject: `wysuń`** · GNOME ("Wysuwa"); imperative label "Wysuń". macOS AppKit "Eject" key not populated in the pile,
  but "wysuń" is the established term. `high`.
- **rename: `zmień nazwę`** · GNOME ("Zmiana nazwy"); imperative "Zmień nazwę". `high`.
- **sort: `sortuj`** · GNOME ("Porządkowanie"/"Sortowanie"); imperative "Sortuj". `high`.
- **sidebar: `panel boczny`** · GNOME ("Panel boczny"). `high`.
- **bookmark: `zakładka`** · GNOME ("zakładka"). Plural "zakładki". `high`.
- **disconnect: `rozłącz`** · macOS AppKit ("Rozłącz"). `high`.
- **info / get info: `informacje`** · macOS Finder ("Informacje"). `high`.

Tentative / needs a native check:

- **volume: `wolumin`** · no clean macOS "volume" string in the pile; "wolumin" is the standard Polish technical term
  for a mounted volume, "partycja" for partition. `tentative`.
- **tab (UI tab): `karta`** · standard Polish UI for tabs (MS/GNOME convention); never "tabulator" (that's the Tab key).
  `tentative`.
- **pane: `panel`** · the two file lists are "panele"; no direct macOS "pane" string. `tentative`.
- **listing: `lista plików`** · reads natural for the file list; no single canonical source term. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. macOS UI names Cmdr opens into should match what a Polish macOS shows
("Kosz", "Ustawienia").

## Plurals

CLDR categories for `pl`: `one`, `few`, `many`, `other` (verified with `new Intl.PluralRules('pl')`; GNOME's
nplurals=3 collapses few+other differently, use the four CLDR categories, not GNOME's three). Write all four.

- **one**: integer 1 only (`i=1, v=0`). "1 plik".
- **few**: integers ending 2–4 except 12–14 (`i%10=2..4` and `i%100≠12..14`). "2 pliki", "23 pliki".
- **many**: integers ending 0,1,5–9 and 12–14 (`i%10=0..1,5..9` or `i%100=12..14`), plus 0 and large numbers.
  "5 plików", "11 plików", "0 plików", "100 plików".
- **other**: decimals/fractions (`v≠0`). "1,5 pliku".
- **Trap: `one` is integer 1 only; "21", "31" do NOT take `one` (they're `few`).** And `many` is the big bucket here
  (5+), unlike Slovak/Czech where `many` is the decimal bucket. Keep article/adjective agreement inside each branch
  (case follows: 1 = nominative sg, 2–4 = nominative pl, 5+/0 = genitive pl). The `desktop-i18n-plural` check requires
  all four.

## Notes and decisions

- **Quotation marks: `„…"`** (low-9 opening U+201E, high-6 closing U+201D), the standard Polish form. Nested:
  guillemets `»…«` (inward) or French `«…»` depending on house style; prefer `»…«`. Avoid straight ASCII `"` and English
  `"…"`.
- **Numbers and dates come from the formatter layer.** Polish uses a comma decimal and space (non-breaking) thousands
  separator (1 000); `formatNumber()`/`formatBytes()` produce these from the locale. Never hardcode separators.
- **Length.** Polish runs longer than English (case endings, compounds, ~20-30% expansion), so overflow-check the
  layout against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
