# Azerbaijani (az) translation style guide

Working notes for translating Cmdr into Azerbaijani. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Azerbaijani.

**Sparse pile, no macOS.** Apple ships no Azerbaijani macOS UI. The pile has GNOME Nautilus for `az`, plus an `az-Latn`
folder with a Microsoft terminology glossary and a Microsoft style guide (`_ignored/i18n/az/` and `az-Latn/`). No Xfce,
no macOS. Terms lean on GNOME + Microsoft. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. The first is the headline.

- **Script: RESOLVED to Latin (`az-Latn`).** Azerbaijani in Azerbaijan switched from Cyrillic to a Latin alphabet
  (officially completed 2001); modern Azerbaijan uses Latin exclusively. The pile's Microsoft sources are `az-Latn`.
  Perso-Arabic `az-Arab` (Iran) is RTL and out of scope under the no-RTL decision. Don't use Cyrillic (it reads as
  dated/Soviet-era to a modern reader). See the script decision point below and
  [`script-decisions.md`](../script-decisions.md). No longer open.
- **Address form: polite plural "Siz" recommended, worth a sign-off (high).** Azerbaijani (Turkic, like Turkish) has a
  T-V split; software uses the polite plural. MS Azerbaijani uses the polite second person ("istəsəniz" = if you wish,
  verified 2026-06-20). Recommended default below.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. MS Azerbaijani follows the general
Microsoft voice ("warm and relaxed, less formal", and "casually and politely asks the user", verified 2026-06-20), so
keep it modern and plain, just polite. With no macOS reference, prioritize clear, plain Azerbaijani. Error messages stay
calm and actionable: phrase the problem and the next step, and avoid "xəta" (error) / "alınmadı" (failed) as a bare
status label the way English avoids "error"/"failed".

## Formality

Azerbaijani distinguishes informal singular "sən" from polite plural "Siz", like Turkish.

- **Polite plural "Siz", throughout. Never informal "sən".** Carried by the plural verb ending (`-sınız`/`-siniz`).
- **Action labels (buttons, menu items): imperative.** GNOME Azerbaijani uses imperatives: "Aç" (Open), "Yenidən
  adlandır" (Rename), "Ləğv et" (Cancel) (GNOME Nautilus, verified 2026-06-20). The bare 2nd-person imperative is the
  standard Turkic label form. So the rule: **labels = imperative; sentences to the user = polite plural "Siz"; never
  informal "sən".** Confidence: high.
- Note the macron-less dotted/dotless i distinction (i vs ı, İ vs I) is phonemic in Azerbaijani; getting it wrong
  changes words. Ensure the catalog and any uppercasing respect Azerbaijani casing rules (the Turkic i-casing trap).

## Decision points

- **Script: RESOLVED to Latin (`az-Latn`).** Recorded in [`script-decisions.md`](../script-decisions.md).
- **Regional variant: `az` / `az-Latn` (Azerbaijan).** The Republic of Azerbaijan standard, Latin script. The Iranian
  (South Azerbaijani, Perso-Arabic) variant is a separate, RTL workstream - out of scope by default. Confidence: high.
- **Gender / inclusive language: a non-issue (high).** Azerbaijani (Turkic) has NO grammatical gender and a single
  gender-neutral 3rd-person pronoun ("o" for he/she/it). User-gender-agreement problems don't arise. No special handling
  needed. Confidence: high.
- **Vowel harmony in suffixes (a Turkic gotcha, high).** Azerbaijani suffixes (case, plural, possessive) harmonize with
  the stem vowel, so a suffix attached to an inserted `{path}` or token can't be chosen generically. Structure sentences
  so suffixes never attach to an uncontrolled insert; phrase around it. Confidence: high.
- **Capitalization: sentence case everywhere (high).** Azerbaijani capitalizes only the first word and proper nouns.
  Matches Cmdr's sentence-case rule. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/az/` (GNOME Nautilus) and
`az-Latn/` (MS terminology, MS style guide; NO macOS) on 2026-06-20. Sources decide the term; Cmdr writes its own value
(MS copyrighted, GNOME GPL, never copied verbatim). Without macOS, terms are `high` where GNOME and MS agree, else
`tentative`.

- **trash: `zibil` (qutusu)** · GNOME ("Zibil"). The bin is commonly "zibil qutusu". `high`.
- **open: `aç`** · GNOME ("Aç"). `high`.
- **rename: `yenidən adlandır`** · GNOME ("Yenidən adlandır"). `high`.
- **cancel: `ləğv et`** · GNOME ("Ləğv et"). `high`.
- **folder: `qovluq`** · standard Azerbaijani; verify against MS terminology. `tentative`.
- **file: `fayl`** · standard borrowing; verify. `tentative`.
- **search: `axtarış`** · standard term; verify against GNOME. `tentative`.
- **volume / pane / tab / bookmark** · no clean source; defer to a native reviewer using the MS `az-Latn` glossary.
  `tentative`.

Add terms as they come up; triangulate GNOME + the MS `az-Latn` glossary, and record confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.ts`. When a Turkic suffix
must attach to a brand token, Azerbaijani uses an apostrophe ("macOS'da") - that's orthography, not translation, but
mind the ICU apostrophe-doubling rule below.

## Plurals

CLDR categories for `az`: `one`, `other` (verified with `new Intl.PluralRules('az')`). Only two forms. Note: Turkic
languages drop the plural suffix on the counted noun after a number ("3 fayl", not "3 fayllar"), so the `other` branch
uses the singular noun form. Write the singular form inside the count message. The `desktop-i18n-plural` check requires
both categories.

## Notes and decisions

- **Quotation marks: `«…»`** (guillemets are the common Azerbaijani form), with `„…"` also seen. Confirm with a native
  reviewer; avoid straight ASCII `"`.
- **Numbers and dates come from the formatter layer.** Azerbaijani uses a comma decimal and a period/space thousands
  separator; `formatNumber()`/`formatBytes()` produce locale-correct output. Never hardcode separators.
- **Casing trap.** The dotted/dotless i (i/ı, İ/I) makes naive uppercase/lowercase wrong; never transform case in a
  string by hand, and verify any code-level casing uses Azerbaijani locale rules.
- **Length.** Agglutinative suffixing makes some strings longer; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) - relevant because
  Azerbaijani uses apostrophes on suffixed brand tokens - and keep every `{placeholder}` and `<tag>` verbatim. Full
  rules: the agent-handoff block in [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/az/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
