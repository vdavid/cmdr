# Hebrew (he) translation style guide

Working notes for translating Cmdr into Hebrew. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Hebrew.

**RTL, and a real macOS reference exists.** Apple ships a Hebrew macOS UI, so the pile has macOS Finder/AppKit
(highest authority) plus MS terminology, MS style guide, GNOME Nautilus, and Xfce Thunar (`_ignored/i18n/he/`). RTL is a
layout workstream, not just translation. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. The first two are real open flags; the rest carry a confident default.

- **RTL readiness gates shipping Hebrew at all (high).** This is the headline. Hebrew needs full UI mirroring (panes
  swap sides, chevrons and back/forward arrows reverse, progress fills reverse). It's an app-code workstream separate
  from translation, shared with any future Arabic/Persian/Urdu locale. Don't ship Hebrew until the app's RTL layout AND
  bidi isolation are verified end to end. See the RTL decision point.
- **Gender-neutral strategy for user-facing verbs (high).** Hebrew marks gender on verbs and adjectives, so "you
  deleted" / "are you sure" force a gender guess about the user. Microsoft's Hebrew guide adopts an explicit
  gender-neutrality approach (verified 2026-06-20). Cmdr should commit to one neutral strategy app-wide (see the gender
  decision point); flagging because it shapes every sentence the app speaks to the user.
- **Numerals stay Western (0-9) (high, but worth a sign-off).** Hebrew uses Western Arabic numerals in everyday and
  software contexts (Hebrew letter-numerals are ceremonial only). A file manager shows counts and sizes constantly;
  Western digits are correct. Low-risk default, flagged only because it's a visible global choice.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. The Hebrew Microsoft voice is "warm and
relaxed, less formal", and explicitly says to "avoid artificial high register" and instead "think how would I say it to
my friend, partner, parent" (MS style guide, verified 2026-06-20). That matches Cmdr's warm-informal English well.
Error messages stay calm and actionable: phrase the problem and the next step, and avoid "שגיאה" (error) / "נכשל"
(failed) as a bare status label the way English avoids "error"/"failed".

## Formality

Hebrew has no T-V politeness split the way Slavic or Romance languages do; register is carried by word choice and verb
form, not a separate polite pronoun. The Microsoft Hebrew guidance is specific and useful:

- **Instructions to the user: plural imperative, not infinitive.** MS Hebrew: "for instructions, do not use the
  infinitive form, but rather informal imperative" in the plural ("לחצו על…" press, "הקישו על…" tap) (verified
  2026-06-20). The plural form also doubles as the gender-neutral address (see below).
- **Button and menu labels: the gerund (verbal noun), not the imperative.** MS Hebrew: "in UI elements such as button
  labels, the gerund should be used instead of the imperative or infinitive" (verified 2026-06-20). macOS Hebrew matches
  this exactly: AppKit shows "העתקה" (copying = Copy), "מחיקה" (deletion = Delete), "פתיחה" (opening = Open), "ביטול"
  (cancellation = Cancel). So the rule is dual: **standalone labels = gerund/verbal noun; sentences/instructions to the
  user = plural imperative.** Confidence: high (macOS and MS agree).
- **Address: second-person plural.** MS Hebrew uses "direct second person plural" for addressing readers ("אתם יכולים",
  "אם תלחצו"), which is also the device for gender-neutrality. Use plural address throughout. Confidence: high.

## Decision points

### RTL: the dominant concern

Hebrew is written right-to-left. This is the single biggest issue and it's a LAYOUT concern as much as a text one
(shared with any future Arabic/Persian/Urdu locale):

- The whole UI must mirror: panes swap sides, cursor/selection logic, progress bars, chevrons, and back/forward
  navigation arrows all reverse. A right-pointing "forward" arrow is wrong in RTL.
- Cmdr is a two-pane file manager, so the left/right pane mental model itself mirrors under RTL. Confirm the app's
  layout engine flips correctly before shipping any RTL locale; this is an app-code question, not a translation one.
- **Bidi hazard: file paths, URLs, brand names (Cmdr, macOS, SMB), and Western numbers are LTR runs embedded in RTL
  text.** Without Unicode bidi isolation, a `{path}` insert visually scrambles the surrounding sentence, and multi-digit
  numbers can render reversed. Ensure every `{path}`, `{count}`, `{size}`, and brand token is bidi-isolated (Unicode
  FSI/PDI or equivalent).
- Recommendation: do NOT ship Hebrew until RTL mirroring AND bidi isolation are verified end to end. Confidence: high
  that RTL is the gating issue.

### Gender: the second major concern

Hebrew has grammatical gender on verbs, adjectives, and 2nd-person forms; there is no neutral gender. Addressing a
single unknown user forces a masculine/feminine choice. Microsoft's Hebrew guide adopts an interim gender-neutrality
approach (verified 2026-06-20):

- Use gender-neutral alternatives for nouns and verbs; avoid compounds with gender-specific terms.
- For addressing the user, use the **plural form of address** ("אתם", plural imperatives), which reads as neutral, and
  rewrite generic references to drop gendered pronouns.
- Prefer the **gerund/verbal noun** for UI verbs (button labels), which sidesteps gender entirely ("מחיקה" rather than a
  gendered imperative).
- Recommendation: combine plural address for sentences + gerund for labels + impersonal/nominal phrasing for system-state
  messages. This covers nearly all of Cmdr's strings without a gender guess. Confidence: high that this is the right
  strategy; the only open call is committing to it app-wide.

### Numerals: Western digits

Hebrew uses Western Arabic numerals (0-9) in everyday and software contexts; Hebrew letter-numerals (gematria) are
ceremonial. Let `Intl` with the `he` locale shape numbers. Recommendation: Western digits throughout; bidi-isolate them
inside RTL sentences. Confidence: high.

### Regional variant: one, `he` (`he-IL`)

Hebrew is standardized only in Israel; no second national standard, no variant matrix. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/he/` (macOS Finder/AppKit, MS
terminology, GNOME Nautilus, Xfce Thunar) on 2026-06-20; macOS strings cited are what Hebrew Finder/AppKit actually show.
Sources decide the term; Cmdr writes its own value (Apple/MS copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (sources agree):

- **folder: `תיקיה`** · macOS Finder ("תיקיה"), GNOME. `high`.
- **file: `קובץ`** · macOS, GNOME. `high`.
- **trash: `פח אשפה` (full) / `פח` (short)** · macOS Finder maps "Trash"→"פח אשפה" and "Bin"→"פח". Use "פח אשפה" for the
  destination name, "פח" where space is tight. `high`.
- **eject: `הוצא` / gerund `הוצאה`** · macOS Finder. `high`.
- **cancel: `ביטול`** (gerund) · macOS AppKit. `high`.
- **copy: `העתקה`** (gerund label) · macOS AppKit ("העתקה"). `high`.
- **delete: `מחיקה`** (gerund label) · macOS AppKit ("מחיקה"). `high`.
- **open: `פתיחה`** (gerund label) · macOS AppKit ("פתיחה"). `high`.
- **save: `שמירה`** (gerund label) · macOS AppKit ("שמירה"). `high`.
- **disconnect: `התנתקות`** · macOS AppKit ("התנתקות"). `high`.
- **search: `חיפוש`** · macOS Finder ("חיפוש ב-Finder"). `high`.
- **network: `רשת`** · macOS Finder ("רשת"). `high`.
- **shared: `משותף`** · macOS Finder ("משותף"). `high`.

Add `tab`, `pane`, `volume`, `bookmark`, `listing` as they come up; triangulate macOS first, then MS/GNOME, and record
confidence. Note RTL bidi isolation for any term sitting next to a `{path}` or brand token.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. These Latin tokens are LTR runs inside RTL text: bidi-isolate them.

## Plurals

CLDR categories for `he`: `one`, `two`, `other` (verified with `new Intl.PluralRules('he')`). Note the distinct **two**
(dual) category: Hebrew grammatically marks a dual, so count messages must write a `two` branch in addition to one/other.
This is unusual (most languages lack `two`) and easy to miss. The `desktop-i18n-plural` check requires every plural
message to cover all three.

## Notes and decisions

- **Punctuation and bidi.** Hebrew uses the same `.,?!` marks, but inside RTL text their visual position flips; rely on
  the bidi algorithm rather than reordering characters by hand. Hebrew quotation marks are typically the gershayim-style
  `"…"` or standard double quotes; avoid mixing.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` produce locale-correct output;
  never hardcode separators or digit shapes in a string.
- **Length.** Hebrew is usually compact (no case endings, no articles split out), so overflow is less of a risk than RTL
  mirroring, but still overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
