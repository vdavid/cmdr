# Slovenian (sl) translation style guide

Working notes for translating Cmdr into Slovenian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Slovenian.

## Decisions to confirm with David

These are the calls a translator can't make alone. The rest of this guide assumes them; the address-form one is the real
flag, the rest carry a confident default and are listed so they're never relitigated.

- **Address form: RESOLVED to informal `ti`** (consumer-brand evidence; see Formality and
  [`formal-informal-decisions.md`](../formal-informal-decisions.md)). No longer open. Slovenian still prefers
  impersonal, agentless phrasing where natural (it stays gender-neutral too), but direct second-person address is
  informal `ti`, never `vikanje`.
- **Quotation-mark house style: `»…«` vs `„…"` (high either way).** Both are standard Slovenian (see Notes); the choice
  is a house-style pick, not a correctness one. Recommended default: `»…«` (most traditional, most distinctively
  Slovenian); `„…"` is fully acceptable. Pick one and be consistent.

## Voice and tone

Friendly, concise, active, calm, and **informal in address** (`ti`; see Formality). Still prefer impersonal,
present-tense, agentless constructions where they read naturally ("Datoteka je premaknjena", "Kopiranje je končano"):
they read tight and stay gender-neutral. Where the user is addressed directly, use informal `ti`. Error messages stay
calm and actionable: phrase the problem and the next step, and don't use "napaka" (error) or "spodletelo" (failed) as a
status label the way English avoids "error"/"failed".

## Formality

**Verdict: informal `ti` (tikanje), not `vikanje`.** Consumer brands (IKEA, Spotify, Netflix, and peers; IKEA-SI uses
informal `ti`/`Vnesi`) address Slovenian users informally, which fits Cmdr's friendly personal voice. Formality decision
recorded in [`formal-informal-decisions.md`](../formal-informal-decisions.md). Slovenian still leans on impersonal,
agentless phrasing where it reads naturally (it also stays gender-neutral), but where the user is addressed in the
second person, the register is informal `ti`, never `vikanje`.

- **Direct address: informal `ti`.** "Ali si prepričan/-a?" (Are you sure?), "Ali želiš shraniti spremembe?" (Do you
  want to save changes?). Prefer an impersonal recast where it avoids a gendered participle ("Ali so spremembe
  shranjene?").
- **Action labels (buttons, menu items): short infinitive/neutral command form.** This is what macOS Slovenian shows:
  "Shrani" (Save), "Prekliči" (Cancel), "Izbriši" (Delete), "Odpri" (Open), "Kopiraj" (Copy). These short forms read as
  labels and align with the `ti` register. (verified against the reference pile, 2026-06-20: macOS Finder shows
  "Prekliči", "Izvrzi", "Odpri mapo", all short command form.)

## Decision points

- **Script: Latin, no decision.** Slovenian uses the Latin alphabet with č, š, ž (and the rarer đ). No script choice.
  Confidence: high.
- **Regional variant: one, `sl` (`sl-SI`).** Slovenian-speaking minorities in Italy, Austria, and Hungary don't get
  separate software locales. No variant matrix. Confidence: high.
- **THE DUAL: the single most important decision point (high).** Slovenian is one of few living languages with a
  productive grammatical **dual**. Its CLDR cardinal categories are **`one`, `two`, `few`, `other`** (no `zero`, no
  `many`). See Plurals below for the exact rules. The trap: an English string has at most two forms ("1 file" / "N
  files"), but Slovenian needs **four distinct strings**, and the dual (`two`) is a separate noun inflection AND a
  separate verb form, not just a different number word. "2 files selected" is "Izbrani sta **2 datoteki**" (dual verb
  _sta_, dual noun _datoteki_), distinct from "Izbrane so **3 datoteke**" (few) and "Izbranih je **5 datotek**" (other).
  You can't author the plural string and swap the number: the grammar around the number changes. Recommended default:
  key plurals by CLDR category name (ICU MessageFormat / Fluent do this), author all four forms, and have a native
  reviewer confirm the dual carries through to the surrounding verb and noun, not just the count.
- **Gender / inclusive language: prefer impersonal to dodge it (high).** Slovenian gender-marks past-tense verbs and
  adjectives, and these have dual forms too. "You have deleted the file" forces a gender (and number) on the user.
  Standard UI handling: **avoid gendered constructions** with impersonal/present-tense phrasing ("Datoteka je izbrisana"
  rather than "Izbrisali ste…"). This is the same move that solves formality, so one choice fixes both. Where a gendered
  form is unavoidable, masculine is the conventional unmarked/generic; slash-pairs ("naredil/a") are for formal
  documents, not app UI. Recommended default: impersonal, agentless, present-tense throughout.
- **Capitalization: sentence case everywhere (high).** Slovenian capitalizes only the first word and proper nouns;
  English title case does not exist in Slovenian ("Nova mapa", not "Nova Mapa"). Matches Cmdr's existing rule with no
  friction.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Confidence is `confirmed` (a native human signed off), `high`
(authoritative sources agree), or `tentative` (sources conflict or none had it). Evidence verified against the reference
pile (`_ignored/i18n/sl/`: macOS Finder/AppKit, MS terminology, GNOME Nautilus, Xfce Thunar) on 2026-06-20; macOS
strings cited are what Slovenian Finder/AppKit actually show. Sources decide the term; Cmdr writes its own value
(Apple/MS copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (sources agree):

- **folder: `mapa`** · macOS Finder ("Mapa", "Ustvari mapo", "Deljena mapa"), GNOME. Accusative "mapo", plural "mape".
  `high`.
- **file: `datoteka`** · macOS Finder/AppKit ("Datoteka"), GNOME. Dual "datoteki", plural "datoteke", genitive pl
  "datotek". `high`.
- **directory: `imenik`** · MS terminology; use only where the technical filesystem sense matters, else "mapa". `high`.
- **trash: `koš`** · macOS Finder ("Koš"), GNOME ("koš"). `high`.
- **move to trash: `premakni v koš`** · GNOME ("Premakne v koš"), aligns with macOS "Koš". `high`.
- **delete (permanent): `izbriši`** · macOS AppKit ("Izbriši"). Reserve for the destructive delete; use "premakni v koš"
  for the safe move. `high`.
- **eject: `izvrzi`** · macOS Finder ("Izvrzi"), GNOME ("Izvrzi"). `high`.
- **copy: `kopiraj`** · macOS AppKit ("Kopiraj"). `high`.
- **cancel: `prekliči`** · macOS AppKit ("Prekliči"). `high`.
- **open: `odpri`** · macOS AppKit ("Odpri", "Odpri mapo"). `high`.
- **save: `shrani`** · macOS Finder ("Shrani mapo…"). `high`.
- **overwrite: `prepiši` (verb) / `prepis` (noun)** · macOS Finder ("Prepis pripon", "Prepis na cilju", "naj se …
  prepiše"). `high`.
- **search: `iskanje`** · macOS Finder ("Iskanje"). `high`.
- **server: `strežnik`** · macOS Finder ("Povezani strežniki"). Connect verb "Poveži". `high`.
- **disconnect: `prekini povezavo`** · macOS AppKit ("Prekini povezavo"). `high`.
- **tab (UI tab): `zavihek`** · macOS AppKit maps "Tab" to "zavihek" (the UI tab; "Tabulator" is the keyboard key).
  `high`.
- **sidebar: `stranska vrstica`** · GNOME ("Stranska vrstica"). `high`.
- **bookmark: `zaznamek`** · GNOME ("Ustvari zaznamek"). Plural "zaznamki". `high`.
- **sort: `razvrsti`** · GNOME ("Razvrsti"). `high`.

Tentative / needs a native check:

- **volume: `nosilec`** · macOS AppKit ("Nosilec" = volume), the macOS-backed term for a mounted disk volume.
  `tentative` (only the AppKit string backs it; worth a native check that it reads natural for an SMB/MTP mount).
- **pane: `podokno`** · no direct macOS "pane" term; Slovenian UI uses "podokno" for a sub-window region; the two file
  lists are "podokni" (dual). `tentative`.
- **listing: `seznam datotek`** · GNOME renders "List View" as "Seznamski pogled"; "seznam datotek" reads natural for
  the file list. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`. macOS UI names Cmdr opens into should match what a Slovenian macOS shows
("Koš", "Nastavitve").

## Plurals

CLDR categories: `one`, `two`, `few`, `other` (verified with `new Intl.PluralRules('sl')`; the GNOME catalog's gettext
rule is the matching 4-form dual rule). Write all four. Boundaries are mod-100, not "small vs large" (`v` = visible
fraction digits, `i` = integer part):

- **one**: `v=0 and i mod 100 = 1` → 1, 101, 201, 301 … "1 datoteka" (nominative sg).
- **two**: `v=0 and i mod 100 = 2` → 2, 102, 202 … "2 datoteki" (**dual** noun, dual verb _sta_).
- **few**: `v=0 and i mod 100 = 3..4`, **or any decimal** (`v≠0`) → 3, 4, 103, 104 … plus 0.0, 1.5, 10.0 … "3 datoteke"
  (paucal).
- **other**: everything else → **0**, 5–19, 20, 100, 1000 … "5 datotek", "0 datotek" (genitive pl).
- **Traps:** (1) the dual (`two`) changes the noun inflection AND the surrounding verb, not just the number; (2)
  boundaries are mod-100, so 101→one, 102→two, 103/104→few, 105→other (a naive "1 = singular, else plural" gets large
  numbers wrong); (3) **0 is `other`**, not a special zero category, same form as "5 datotek". The `desktop-i18n-plural`
  check requires every plural message to cover all four.

## Notes and decisions

- **Quotation marks: `»…«` (recommended) or `„…"`.** Slovenian, like German, uses low-opening/high-closing, never
  English high-high. `»besedilo«` (guillemets pointing **inward**, opposite of French `«…»`; U+00BB … U+00AB) is the
  most traditional and most distinctively Slovenian; `„besedilo"` (U+201E … U+201C) is also standard. Nested: `›…‹` or
  `‚…'`. Avoid straight ASCII `"` and English `"…"`. See the Decisions flag above.
- **Numbers and dates come from the formatter layer.** Slovenian uses a comma decimal and dot/space thousands separator;
  `formatNumber()`/`formatBytes()` produce these from the locale. Never hardcode separators in a string.
- **Length.** Slovenian runs somewhat longer than English (case endings, the dual), so overflow-check the layout against
  the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/sl/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
