# Limburgish (li) translation style guide

Working notes for translating Cmdr into Limburgish. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Limburgish.

Low-priority signal is strong here (see Decision points): treat li as bottom-of-backlog. The only reference is one aged,
partly-fuzzy GNOME Nautilus catalog (`li/gnome-nautilus/nautilus.po`, 2003, single translator, ~59% clean / ~32% fuzzy,
no Plural-Forms header). No macOS (Tier 1), no Microsoft (Tier 2). Usable only as a loose term-sense reference, not
authority.

## Voice and tone

Friendly, concise, active, calm, if li is built at all. Error messages stay calm and actionable.

## Formality

Formal/polite second person `Geer` / `geer` (cognate of Dutch U/gij), with the matching plural verb (e.g. "Geer kènt
gein map verplaatse"), per the GNOME catalog; informal `doe`/`diech` forms don't appear. Matches Dutch-style polite
address.

## Decision points

Ship li at all, or fall back to Dutch? (THE strategic call for David):
- Options: (a) ship li; (b) skip li, resolve to nl (Dutch).
- Limburgish is a recognized regional language of the Dutch/Belgian Limburg provinces (Netherlands, 1997, European
  Charter Part II), ~1.2-1.5M speakers, but speakers use Dutch for computing essentially universally. No major vendor
  (Apple, Microsoft, Google, Spotify, Netflix) ships a Limburgish UI; li surfaces only in community/open-source contexts
  (the aged GNOME catalog; install-language requests that go unmet).
- Recommendation: skip li; fall back to Dutch (nl). Near-zero demand, no standard orthography, no authority sources,
  fully Dutch-fluent audience. li is a passion/identity play, not a coverage gap. If ever done, do it last and only as a
  community-contributed, native-reviewed extra once nl ships. Confidence: high.

Orthography / standardization (only if li is built):
- Limburgish has NO single standardized orthography (it's a dialect continuum). The main candidate is the Veldeke
  "Spelling 2003" (used by the Province and on bilingual place-name signs), but it has no official status.
- Majors: none, no vendor has made this choice, so no precedent to inherit.
- Recommendation: if built, target Veldeke Spelling 2003 with a native Veldeke-literate reviewer; do NOT copy the 2003
  Nautilus catalog's ad-hoc spelling wholesale. Confidence: high on the target; medium on consistency without review.

Fallback chain:
- Recommendation: `li → nl → en`. Dutch is the natural, universally-understood second language for every Limburgish
  speaker; never fall straight to English. Confidence: high.

## Terminology and glossary

From the 2003 Nautilus catalog (loose reference only; heavy dialect-specific spelling). Format:
`English → chosen · source · confidence`.

- file → besjtandj (pl. besjtenj) · GNOME · tentative
- folder → map (pl. mappe) · GNOME · tentative
- copy → kopiëre · GNOME · tentative
- paste → plekke · GNOME · tentative
- open → Äöpene · GNOME · tentative
- cancel → Annulere · GNOME · tentative
- rename → Herneume · GNOME · tentative
- delete/remove → ewegdoon / wösje · GNOME · tentative
- trash → Papeerkörf · GNOME · tentative

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `other` (standard Germanic, same shape as Dutch/English: `one` when n=1, `other` otherwise). The
`desktop-i18n-plural` check requires every plural message to cover the categories this language needs.

## Notes and decisions

- Heavy Limburgish-specific spelling (sj-clusters, ä/ö, glottal markers) in the only catalog; an orthography target must
  be fixed before any consistent work.
- Numbers and dates come from the formatter layer. Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

- The big one: ship li at all, or just fall back to nl? (Recommendation: skip; fall back to Dutch.)
- If shipping: target orthography (Veldeke Spelling 2003) and a native reviewer.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in [`../guides/i18n-translation.md`](../guides/i18n-translation.md)
and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
