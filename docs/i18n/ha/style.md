# Hausa (ha) translation style guide

Working notes for translating Cmdr into Hausa (Hausa / هَوُسَ). Read [`README.md`](../README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes
carry into Hausa.

Sourced: the pile has GNOME Nautilus for the base `ha` (about 1,200 translated strings, Tier 3) and MS terminology under
the variant `ha-Latn-NG` (Tier 2); run `ls _ignored/i18n/ | grep '^ha'` to see both. No macOS, no MS style guide.
Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

- **Script: Latin (Boko), recommended; the Ajami (Arabic) script exists but isn't the localization target (high).**
  Hausa is written mainly in a Latin-based orthography called Boko (the basis of the `ha-Latn-NG` tag and all the
  software localization in the pile), and historically in an Arabic-based script called Ajami (RTL). All software
  sources here are Latin. Recommendation: **target Latin/Boko** (`ha`). Don't pursue Ajami unless specifically asked; it
  would be a separate RTL locale. Confidence: high.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Lean on the GNOME Hausa catalog for established file-manager phrasing
and MS Hausa terminology for terms. Hausa is genderless in the relevant second-person address (see Decision points),
keeping things simple. Error messages stay calm and actionable: name the problem and the next step, and avoid a bare
"kuskure" (error) status label, consistent with Cmdr's English voice.

## Formality

- **Direct second person; Hausa has no T/V politeness split.** Hausa doesn't distinguish a formal vs informal "you" the
  way European languages do (the second-person pronouns vary by gender and number: `kai` masc, `ke` fem, `ku` plural,
  not by politeness). Software addresses the user directly. For gender-neutral address, prefer the plural `ku` ("you
  all") or impersonal phrasing to avoid picking masculine/feminine singular (see Decision points). Confidence: high.
- **Action labels (buttons, menu items): the established imperative form** from the GNOME catalog ("Soke" Cancel). Keep
  labels short. Confidence: medium-high.

## Decision points

- **Script: Latin/Boko, no decision for the shipped locale (high).** See Decisions to confirm. Boko uses a few special
  letters (ɓ, ɗ, ƙ, and an apostrophe-marked `ʼy`). Note the hooked letters and the apostrophe are orthographic, not ICU
  escapes, but the apostrophe still needs ICU doubling in catalog values (see Notes). Confidence: high.
- **Regional variant: target `ha` / `ha-Latn-NG` (Nigeria).** Hausa is spoken across Nigeria and Niger; the Niger
  orthography differs slightly, but Nigeria (`ha-Latn-NG`, the MS tag) is the largest and the localization standard.
  Target the base `ha` with Nigerian conventions; no separate variant needed for now. Confidence: high.
- **Gender / inclusive language (medium problem, clean fix).** Hausa grammar is strongly gendered: the singular "you"
  splits masculine (`kai`) vs feminine (`ke`), and verbs/adjectives agree. Addressing a user of unknown gender in the
  singular forces a guess. **Fix: use the plural `ku` for "you", and/or impersonal phrasing**, which sidesteps the
  masculine/feminine split. This is the recommended default and the same move the gendered European languages use.
  Confidence: high on the problem and the fix.
- **Capitalization: sentence case (high).** Latin Hausa has case; capitalize only the first word and proper nouns in
  labels and titles. English title case is wrong. Confidence: high.
- **Tone marks usually omitted in writing.** Hausa is tonal and has long/short vowel distinctions, but everyday writing
  (and software) omits the tone/length diacritics. Follow the GNOME catalog: plain Boko without tone marks. Confidence:
  high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/ha/` (GNOME Nautilus) and
`_ignored/i18n/ha-Latn-NG/` (MS terminology) on 2026-06-20; no macOS for Hausa. Sources decide the term; Cmdr writes its
own value (MS copyrighted, GNOME GPL, never copied verbatim).

Settled where GNOME and MS agree or a clear single source exists:

- **trash: `Kwandon Shara`** (basket of rubbish) · GNOME ("Trash"). MS gives `kwandon juyawa` (recycle bin sense);
  prefer the GNOME `Kwandon Shara` for the trash-can sense, or settle with native review. `high` (GNOME).
- **cancel: `Soke`** · GNOME ("Cancel" → "Soke"); MS terminology agrees ("cancel" → "Soke"). `high`.
- **folder: `folda`** · MS terminology ("folder" → "folda", a Latinized loan). `high` (MS).
- **file: `Fayil`** · MS terminology ("file" → "Fayil"). `high` (MS).
- **copy: `Kwafi`** · MS terminology ("copy" → "Kwafi"). `high` (MS).
- **delete: `Share`** · MS terminology ("delete" → "Share"). `high` (MS).
- **open: `buɗe`** · MS terminology ("open" → "buɗe"). `high` (MS).
- **save: `Adanawa`** · MS terminology ("save" → "Adanawa"). `high` (MS).

Tentative / needs a native check:

- **eject, rename, network, volume, pane, tab** · not cleanly in the lookups; triangulate GNOME `ha/gnome-nautilus/`
  with MS terminology, else native review. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories for `ha`: `one`, `other` (verified with `new Intl.PluralRules('ha')`, 2026-06-20). The GNOME Hausa
catalog declares `nplurals=2; plural=(n != 1)` (verified 2026-06-20), which agrees: a clean singular/plural split. Write
both.

- **one**: integer 1. "Fayil 1" / "1 fayil" (word order per native phrasing).
- **other**: everything else, including 0 and counts ≥ 2. The `desktop-i18n-plural` check requires both. A native
  reviewer confirms how the counted noun pluralizes (Hausa has plural noun forms but they're often irregular).

## Notes and decisions

- **Apostrophe care.** Boko uses an apostrophe in `ʼy` and as a glottal marker. In catalog values, ICU requires every
  apostrophe doubled (`'` → `''`); make sure orthographic apostrophes are doubled too, or ICU swallows text. See ICU
  mechanics below.
- **Quotation marks:** Hausa UI commonly uses English-style `"…"`; a native reviewer settles house style.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` follow the locale; never
  hardcode separators in a string.
- **Length.** Hausa runs roughly English-length or a bit longer; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ha/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
