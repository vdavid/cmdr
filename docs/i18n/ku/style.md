# Kurdish (Kurmanji) (ku) translation style guide

Working notes for translating Cmdr into Kurmanji Kurdish (Kurdî / Kurmancî). Read [`README.md`](../README.md) for how
this fits the translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice
these notes carry into Kurdish.

`ku` here is **Kurmanji** (Northern Kurdish), written in the **Latin** (Hawar) script. The reference pile has only GNOME
Nautilus for `ku` (about 1,180 translated strings, Tier 3); no macOS, no Microsoft, no Xfce (`_ignored/i18n/ku/`).
Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

- **`ku` means Kurmanji-Latin here; Sorani is a different script and a separate locale (the key flag, high).** "Kurdish"
  is not one writable language: Kurmanji (Northern, mostly Turkey/Syria) uses a **Latin** (Hawar) alphabet, while Sorani
  (Central, mostly Iraq/Iran) uses a **Perso-Arabic, RTL** script and has the tag `ckb`. The pile's `ku` GNOME catalog
  is Kurmanji-Latin. Recommendation: treat `ku` strictly as **Kurmanji-Latin (LTR)**; if Sorani support is wanted, it's
  a separate `ckb` locale with RTL layout, not a variant of `ku`. Flagging so the two are never conflated. Confidence:
  high.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Lean on the GNOME Kurmanji catalog for established file-manager
phrasing; the Kurdish open-source localization community is the de facto terminology authority. Error messages stay calm
and actionable: name the problem and the next step, and avoid a bare "çewtî" (error) status label, consistent with
Cmdr's English voice.

## Formality

- **Direct second person; Kurmanji has a polite/plural form (`hûn`) vs familiar singular (`tu`).** Kurmanji
  distinguishes familiar `tu` from polite/plural `hûn`. Software conventionally uses the polite/plural `hûn` for
  addressing the user, or impersonal phrasing. Recommended default: **`hûn`-register**, consistent with the GNOME
  catalog. Confidence: medium-high (community catalog; no MS style guide to confirm).
- **Action labels (buttons, menu items): the established form** from the GNOME catalog ("Betal" Cancel). Keep labels
  short. Confidence: medium-high.

## Decision points

- **Script: Latin/Hawar, no decision for the shipped locale (high).** See Decisions to confirm. The Kurmanji Latin
  alphabet uses special letters (ç, ê, î, û, ş, plus a dotless/dotted i distinction). Confidence: high.
- **Variant: Kurmanji only (`ku`), distinct from Sorani (`ckb`).** See Decisions to confirm. Within Kurmanji there are
  dialect differences but a common Latin written standard; no internal variant matrix. Confidence: high.
- **Gender / inclusive language (medium problem).** Kurmanji has grammatical gender (masculine/feminine) and an
  ergative-influenced past tense where the verb agrees with the object, plus gendered oblique/izafe forms. This can
  surface gender in sentences. **Fix: prefer impersonal/agentless phrasing** for system messages and lean on the plural
  `hûn` for address. A native reviewer handles agreement. Confidence: medium-high on the problem; standard
  impersonal-phrasing fix.
- **Capitalization: sentence case (high).** Latin Kurmanji has case; capitalize only the first word and proper nouns.
  English title case is wrong. Note the dotless `i` vs dotted `î` distinction (as in Turkish): casing `i`/`I` must
  respect Kurdish/Turkish rules, which the formatter/locale handles, so don't hand-uppercase. Confidence: high.
- **Izafe construction affects placeholder grammar (high, Kurmanji-specific).** Kurmanji links a noun to its modifier
  with an izafe particle that agrees in gender/number/definiteness (`-a`, `-ê`, `-ên`, …). A `{placeholder}` standing
  where an izafe-linked noun goes can't reliably carry the right particle for runtime text. Structure sentences so a
  placeholder doesn't force an izafe agreement on an adjacent fixed word; a native reviewer handles izafe. Confidence:
  high; the subtlest translator-craft concern for Kurmanji.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Only source for `ku` is GNOME Nautilus (Tier 3); the Kurdish
open-source community is the practical terminology authority. Mark single-source terms by how established they are.
Sources decide the term; Cmdr writes its own value (GNOME GPL, never copied verbatim).

Settled from the GNOME Kurmanji catalog:

- **trash: `Çop`** · GNOME ("Trash" → "Çop"). `high`.
- **cancel: `Betal`** · GNOME ("Cancel" → "Betal"). `high`.

Tentative / needs a native check:

- **folder, file, copy, open, delete, rename, eject, network, volume, pane, tab** · the simple GNOME lookups didn't
  return clean single strings; triangulate `ku/gnome-nautilus/` more fully and lean on the wider Kurdish GNOME corpus
  plus native review. Likely candidates: `peldank` (folder), `dosye`/`pelge` (file), confirm before settling.
  `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

CLDR categories for `ku`: `one`, `other` (verified with `new Intl.PluralRules('ku')`, 2026-06-20). The GNOME Kurmanji
catalog declares `nplurals=2; plural=(n != 1)` (verified 2026-06-20), which agrees. Write both.

- **one**: integer 1. "1 pelge".
- **other**: everything else, including 0 and counts ≥ 2. The `desktop-i18n-plural` check requires both. A native
  reviewer confirms the counted-noun form (Kurmanji marks plural via the oblique/izafe system, so the noun form after a
  number isn't a simple `-s`).

## Notes and decisions

- **Dotless/dotted i.** Kurmanji (like Turkish) distinguishes `i`/`î` and casing them is locale-sensitive; rely on
  locale-aware casing, never a blind `toUpperCase()`.
- **Quotation marks:** Kurmanji UI commonly uses `«…»` or English `"…"`; a native reviewer settles house style.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` follow the locale; never
  hardcode separators in a string.
- **Length.** Kurmanji runs roughly English-length or a bit longer; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ku/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
