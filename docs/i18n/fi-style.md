# Finnish (fi) translation style guide

Working notes for translating Cmdr into Finnish. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Finnish.

Well-sourced: the pile has macOS Finder/AppKit (highest authority), MS terminology, MS style guide, GNOME Nautilus, and
Xfce Thunar (`_ignored/i18n/fi/`). Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. None is a hard blocker; the defaults below are confident.

- **Address: impersonal / passive, avoiding a spelled-out "you" (high).** Finnish software convention strongly prefers
  impersonal constructions over direct address. This is the closest thing to a register decision Finnish has (it has no
  formal/informal pronoun split that software uses); recommended default below. Flagged because it sets the tone for
  every sentence.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. MS Finnish is explicit: avoid
"kapulakieli" (bureaucratic stiff language) and "overformal, complex tone"; "word-for-word translation will result in
target text that sounds too formal"; and don't use abbreviations like "esim." for esimerkiksi, they hurt readability
(verified 2026-06-20). So: plain, everyday Finnish, short words, no bureaucratese. Error messages stay calm and
actionable: phrase the problem and the next step, and avoid "virhe" (error) / "epäonnistui" (failed) as a bare status
label the way English avoids "error"/"failed".

## Formality

Finnish HAS a formal plural "te" (teitittely), but software does NOT use it; modern Finnish UI addresses the user with
the singular "sinä" register OR, more commonly, sidesteps direct address entirely with impersonal/passive forms. The
practical rule:

- **Prefer impersonal / passive constructions.** Instead of "Haluatko poistaa nämä tiedostot?" lean on impersonal
  phrasing where natural ("Poistetaanko nämä tiedostot?" = are these files to be deleted). This is the dominant Finnish
  UI register and avoids both an awkward formal "te" and an over-familiar direct "sinä".
- **Where direct address is unavoidable, use singular sinä-register, never the formal teitittely.** Finnish software
  does not teitittele the user; the formal plural reads as stiff customer-service language.
- **Action labels (buttons, menu items): imperative singular OR a verbal noun.** macOS Finnish uses imperatives:
  "Kopioi" (Copy), "Tallenna" (Save), "Poista" (Delete), "Avaa" (Open), "Kumoa" (Cancel/Undo) (macOS AppKit, verified
  2026-06-20). These are the bare 2nd-person-singular imperative, which is the standard label form. So the rule:
  **labels = imperative singular; sentences = impersonal/passive where possible, else singular sinä; never teitittely.**
  Confidence: high.

## Decision points

- **Script: Latin, no decision.** Finnish uses the Latin alphabet plus ä, ö (and å in loanwords/Swedish names). No
  script choice. Confidence: high.
- **Regional variant: one, `fi` (`fi-FI`).** Finnish is standardized only in Finland; no second national standard, no
  variant matrix. (Finland Swedish is `sv-FI`, a different language.) Confidence: high.
- **Gender / inclusive language: a non-issue (high).** Finnish has NO grammatical gender and a single gender-neutral
  3rd-person pronoun ("hän" for he/she). User-gender-agreement problems that plague Slavic/Semitic/Romance languages
  simply don't arise. No special handling needed. Confidence: high.
- **Capitalization: sentence case everywhere (high).** Finnish capitalizes only the first word and proper nouns in
  titles, labels, and buttons. English title case is wrong ("Näytä piilotetut tiedostot", not "Näytä Piilotetut
  Tiedostot"). Matches Cmdr's sentence-case rule. Confidence: high.
- **Compounding and length: the real Finnish gotcha (high).** Finnish writes compounds as single long words
  ("tiedostonhallinta" = file management, "verkkolevy" = network drive) and adds case endings as suffixes, so strings
  run noticeably longer than English and word-break is unforgiving (no spaces inside a compound to wrap on). Overflow-
  check hard against the pseudolocale (`en-XA`). Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/fi/` (macOS Finder/AppKit, MS
terminology, GNOME Nautilus, Xfce Thunar) on 2026-06-20; macOS strings cited are what Finnish Finder/AppKit actually
show. Sources decide the term; Cmdr writes its own value (Apple/MS copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (sources agree):

- **folder: `kansio`** · macOS Finder ("Kansio"). Case-inflects: "kansioon" (to a folder). `high`.
- **file: `tiedosto`** · macOS, MS. Compounds heavily: "tiedostonhallinta". `high`.
- **trash: `roskakori`** · macOS Finder maps both "Trash" and "Bin" to "Roskakori". `high`.
- **eject: `poista käytöstä` / `irrota`** · macOS Finder; "irrota" (detach) for disks. Confirm against macOS Finder
  string for the eject button. `high`.
- **copy: `kopioi`** · macOS AppKit ("Kopioi"). `high`.
- **delete: `poista`** · macOS AppKit ("Poista"). `high`.
- **open: `avaa`** · macOS AppKit ("Avaa"). `high`.
- **save: `tallenna`** · macOS AppKit ("Tallenna"). `high`.
- **cancel: `kumoa`** · macOS AppKit ("Kumoa"). Note: "kumoa" is literally Undo/revoke; verify the Cancel-button
  string against macOS (macOS sometimes uses "Peruuta" for Cancel). `high` on the macOS source, but check the exact
  Cancel-vs-Undo sense.
- **search: `etsi` (verb) / `haku` (noun)** · macOS Finder ("Etsi Finderissa"). `high`.
- **network: `verkko`** · macOS Finder ("Verkko"). `high`.
- **shared: `jaettu`** · macOS Finder ("Jaettu"). `high`.

Add `tab`, `pane`, `volume`, `bookmark`, `listing` as they come up; triangulate macOS first. Mind case endings: the
glossary lists the base form, but the assembled string often needs an inflected form.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Finnish attaches case endings with a colon to non-inflecting tokens
("SMB:n", "macOS:ssa") - that's normal Finnish orthography and does not "translate" the token.

## Plurals

CLDR categories for `fi`: `one`, `other` (verified with `new Intl.PluralRules('fi')`). Only two forms. But note: the
counted noun in Finnish takes the **partitive singular** after a number >1 ("3 tiedostoa", not "3 tiedostot"), so the
`other` branch's noun form differs from the bare plural. Write the partitive form inside the count message. The
`desktop-i18n-plural` check requires both categories.

## Notes and decisions

- **Quotation marks: `"…"`** (both U+201D, the right double quote, used as both opening and closing - the standard
  Finnish form). Avoid straight ASCII `"` and the German-style `„…"`.
- **Numbers and dates come from the formatter layer.** Finnish uses a comma decimal and a space (non-breaking) thousands
  separator (1 000,5); `formatNumber()`/`formatBytes()` produce these. Never hardcode separators in a string.
- **Length.** Long compounds + case-suffix agglutination make Finnish one of the longer-running languages; overflow-
  check carefully.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
