# Khmer (km) translation style guide

Working notes for translating Cmdr into Khmer (ខ្មែរ). Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Khmer.

Sourced: the pile has MS terminology and MS style guide (under `km-KH`), plus a small/dated GNOME Nautilus catalog
(under `km`); no macOS folder for Khmer (`_ignored/i18n/km*`). Evidence verified against the pile on 2026-06-20. The
GNOME catalog is sparse (about 99 translated strings) so MS is the stronger source here.

## Decisions to confirm with David

- **Word-boundary marking with ZWSP is mandatory and is a real authoring burden (the key flag, high).** Khmer does not
  write spaces between words; the MS Khmer style guide is emphatic that you must insert a zero-width space (ZWSP,
  U+200B) at every Khmer word boundary, or line-break and word-break rendering corrupts (verified 2026-06-20). This is
  invisible in the source but load-bearing. Flagging for David because it means every Khmer string a human or agent
  produces has to carry ZWSP at word boundaries, and review tooling/diffs must not strip them. See Decision points.
- **A few file-manager terms (tentative).** No macOS anchor and a thin GNOME catalog; settle core terms against MS
  terminology with a native check.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Khmer has no grammatical gender and no T/V verb conjugation, so the
register is carried by pronoun choice and politeness particles, not by verb forms. Keep sentences short and plain.
Error messages stay calm and actionable: name the problem and the next step, and avoid a bare "កំហុស" (error) status
label the way English avoids "error"/"failed".

## Formality

- **Polite-neutral register.** Khmer politeness is lexical (pronoun and particle choice), not a tu/vous verb split.
  Software conventionally uses a polite-neutral address that avoids both over-familiar and over-deferential pronouns.
  Recommended default: polite-neutral, consistent with MS Khmer's conversational-but-respectful voice. Confidence:
  high. A native reviewer settles the exact pronoun for "you" in UI prompts.
- **Action labels (buttons, menu items): the established verb form.** Follow MS terminology and the GNOME catalog for
  action verbs ("ស្វែងរក" Search, "បិទ" Close appear in the GNOME data); keep labels short. Confidence: medium pending a
  fuller term pass.

## Decision points

- **Script: Khmer, no decision.** Khmer is written in its own abugida script. It is unicameral (no upper/lower case),
  so the title-case-vs-sentence-case question collapses to one letterform. Confidence: confirmed.
- **No inter-word spaces; mark word boundaries with ZWSP (U+200B) (the key technical decision, high).** Khmer text runs
  words together with no space. To get correct line-breaking and word-breaking, the MS Khmer style guide requires a
  zero-width space at every word boundary (verified 2026-06-20). Recommendation: author every Khmer value with ZWSP at
  word boundaries; document this in the translator handoff; and make sure the catalog tooling, the
  `desktop-i18n-*` checks, and review diffs preserve U+200B rather than normalizing it away. Spaces (visible) are still
  used as a date/time separator in Khmer. Confidence: high; this is the single most important Khmer-specific rule.
- **Regional variant: one, `km` (`km-KH`).** Khmer is standardized only in Cambodia; no variant matrix. Confidence:
  high.
- **Gender / inclusive language: a non-issue grammatically.** Khmer has no grammatical gender. Pronoun choice can
  encode social relationship and (sometimes) gender of the speaker/addressee, but UI uses a neutral register, so no
  gender guessing is forced. Confidence: high.
- **Capitalization: not applicable.** Khmer script has no case. Don't try to capitalize labels. Confidence: confirmed.
- **Complex script shaping and font fit.** Khmer stacks subscript consonants and vowel signs, so rendered text can be
  taller and needs a Khmer-capable font; line-height and clipping need a real check, not just width. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/km*` (MS terminology, MS
style guide, a thin GNOME Nautilus) on 2026-06-20; no macOS for Khmer, so MS terminology is the highest available
authority. Sources decide the term; Cmdr writes its own value (MS copyrighted, GNOME GPL, never copied verbatim).
**Author every Khmer value below with ZWSP at word boundaries** even though they're shown here without it.

To settle from MS terminology (`km-KH/microsoft-terminology/`) with a native check:

- **folder** · check MS terminology for the file-system folder sense. `tentative`.
- **file** · MS terminology. `tentative`.
- **trash / move to trash** · MS terminology; confirm the recycle-bin sense. `tentative`.
- **copy, open, cancel, delete, rename, eject** · MS terminology for each action verb. `tentative`.
- **search: `ស្វែងរក`** · appears in the GNOME catalog ("Saved search"). `tentative` (single source).
- **close: `បិទ`** · GNOME catalog. `tentative`.
- **volume / pane / tab** · no macOS anchor; check MS terminology, else leave for native review. `tentative`.

Mark a term `high` only once MS terminology confirms it; the thin GNOME catalog alone is `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand names stay in Latin script inside Khmer text (don't insert ZWSP
inside a Latin brand word).

## Plurals

CLDR categories for `km`: `other` only (verified with `new Intl.PluralRules('km')`, 2026-06-20). Write a single
`other` branch. Khmer doesn't grammatically inflect the noun for number; "1 file" and "5 files" use the same noun form,
with the count and an optional classifier carrying the number. So one branch covers every count: "{count} ឯកសារ". The
`desktop-i18n-plural` check requires the `other` category. A native reviewer confirms classifier usage if a counted
string needs one.

## Notes and decisions

- **Encoding check to add when translating (TODO):** before the `km` catalog ships, wire a ZWSP word-boundary
  validation guard. Khmer has no inter-word spaces, so every `km` value must carry a zero-width space (U+200B) at word
  boundaries for correct line- and word-breaking. The check should flag `km` values that contain no U+200B at all (a
  near-certain sign ZWSP is missing), and the diff/normalization tooling must preserve U+200B rather than strip it.
  Don't insert ZWSP inside Latin brand words. Build this check when `km` is actually translated.
- **ZWSP everywhere (repeat of the decision point because it's that important):** every Khmer value carries U+200B at
  word boundaries. Don't let any pipeline step strip it.
- **Quotation marks:** Khmer commonly uses the French-style guillemets `«…»` or English `"…"` depending on house style;
  a native reviewer settles which. Avoid mixing.
- **Numbers and dates come from the formatter layer.** Khmer has its own digit glyphs (០-៩) but Arabic digits are also
  widely used in UI; `formatNumber()`/`formatBytes()` follow the locale. A visible space or ":" separates date/time
  parts. Never hardcode separators in a string.
- **Length and height.** Khmer can run longer and renders taller (stacked signs); overflow-check both width and
  line-height against the pseudolocale (`en-XA`) and a Khmer font.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
