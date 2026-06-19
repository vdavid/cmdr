# Scottish Gaelic (gd) translation style guide

Working notes for translating Cmdr into Scottish Gaelic (Gàidhlig). Read [`README.md`](README.md) for how this fits the
translation process.

This is the language base (`gd`), the universal Scottish Gaelic set. There's a single written standard (GOC, Gnàthachas
Litreachaidh na Gàidhlig); no region variant is needed. Scottish Gaelic has unusually strong open-source coverage thanks
to long-running community localization (GNOME, Microsoft).

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. The Microsoft Gaelic
style guide explicitly targets a register that's "not overly formal" and consistent with other software. Keep error and
crash copy reassuring and factual.

## Formality

**Address the user with the singular "thu" (informal), recommended, high confidence.** Scottish Gaelic distinguishes
singular "thu" from plural/polite "sibh". Software convention (GNOME, Microsoft) addresses one user with the singular
imperative. The Microsoft style guide stresses keeping a register that's "not overly formal" (it bans formal
slenderisation and the formal "den"), which points to direct singular address. Confidence: high (Microsoft style guide
+ GNOME).

**Imperatives for UI actions** (buttons, menu items): use the imperative, matching the GNOME catalog: "Dèan lethbhreac"
(copy), "Sguab às" (delete), "Sguir dheth" (cancel). Note Gaelic often uses a verbal-noun construction ("Dèan
lethbhreac" = "make a copy") rather than a single verb.

## Decision points

**Coverage is good.** GNOME, Microsoft terminology, and the Microsoft style guide all exist for Scottish Gaelic
(verified 2026-06-20); macOS is missing (Apple does not localize macOS into Gaelic, so the user's Finder chrome is in
English). The Microsoft style guide is detailed and worth reading for grammar conventions. Confidence: confirmed.

- **Script: Latin only, no decision.** Use standard GOC Latin orthography. Confidence: high.
- **Initial mutation (lenition) is the defining Gaelic difficulty.** Scottish Gaelic lenites word-initial consonants
  (shown by an inserted h: "comhad" → "do chomhad") after many triggering words and grammatical contexts. A
  `{filename}`/`{name}` placeholder after a leniting word should lenite, but the catalog can't mutate runtime text.
  Structure sentences so a placeholder sits where no mutation is required, or where leaving it unmutated reads
  acceptably. The Microsoft style guide adds specific rules (no slenderisation in dative/dual to keep the register
  light). Never glue fragments without checking lenition at each join. Confidence: confirmed (Gaelic grammar): the
  biggest blind-translation risk for this language.
- **Plurals: FOUR CLDR categories.** See Plurals below; flagged here because it's a genuine difficulty. The Gaelic rule
  is unusual (it pairs 1/11, 2/12, then 3–19).
- **Gender: Scottish Gaelic has grammatical gender** (masculine/feminine), which triggers lenition and article forms,
  but direct thu-address does not gender the user. Recommendation: direct address, neutral nouns. Confidence: high.
- **Length: Gaelic runs longer than English** (verbal-noun constructions add words, often 20–30%+). Overflow-check tight
  buttons against the pseudolocale (`en-XA`). Confidence: high.

## Terminology and glossary

| English term | Scottish Gaelic | Notes |
| ------------ | --------------- | ----- |
| Copy | Dèan lethbhreac | GNOME (verbal-noun construction) |
| Move | Gluais | GNOME (confirm against catalog) |
| Delete | Sguab às | GNOME ("_Sguab às") |
| Cancel | Sguir dheth | GNOME ("_Sguir dheth") |
| file | faidhle | Microsoft + GNOME (confirm) |
| folder | pasgan | Microsoft + GNOME (confirm) |
| trash | An sgudal | GNOME |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Scottish Gaelic CLDR categories: `one`, `two`, `few`, `other` (verified with `new Intl.PluralRules('gd')`, 2026-06-20;
GNOME nautilus uses a matching 4-form rule `nplurals=4; plural=(n==1||n==11)?0:(n==2||n==12)?1:...`). The rule is
unusual: 1 and 11 share a form, 2 and 12 share a form. Every counted ICU message must write all four branches the
language needs; get the noun form right in each branch. The `desktop-i18n-plural` check enforces coverage. Confidence:
confirmed.

## Notes and decisions

- **Diacritics**: Scottish Gaelic uses the grave accent only (à è ì ò ù), NOT the acute (unlike Irish). This is a
  common cross-Gaelic mistake; keep graves, never substitute acutes.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full
  rules: [`../guides/i18n-translation.md`](../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: Gaelic is a high-effort locale** (four plural forms, lenition). Well-anchored by GNOME + Microsoft, so the
  quality ceiling is high; confirm it's worth the effort before lower-priority locales.
