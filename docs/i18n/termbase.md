# Termbase: schema and tooling

Why: per-locale glossaries grew to 210–375 KB of mostly append-only journal, so translators loaded all of it or grepped
and missed rulings. A structured termbase lets `pnpm i18n:brief` hand a translator only what one batch needs.

The process that uses all this (who translates, the agent prompt): `docs/guides/i18n-translation.md`.

## What the shape defends against

Prose glossaries rotted in three ways; this is how the termbase stands against each, honestly:

- **Dead citations** (a doc naming a catalog key that's gone, or never existed: 91 found across 24 keys when first
  counted): DETECTED. `desktop-i18n-doc-citations` checks the markdown and the termbase JSON, and the termbase check
  holds every `exceptions` key and `decision` pointer to something real.
- **Value drift** (a quoted shipped value that no longer matches the catalog: 20 found across 14 term families):
  REDUCED, not impossible. A ruling stores the chosen FORM, never a per-key value; the brief reads the current values
  from the catalogs, and the drift check flags a key whose translation stopped using the ruling. But `forms`, `note`,
  `exceptions` reasons, and `decisions.md` prose are free text and can still quote a shipped value that later changes.
  Point at keys rather than quoting their values.
- **Self-contradiction** (an append-only entry holding a retired form and its replacement at once, so a translator
  copies the dead one): PREVENTED in `terms.json`, which holds one current ruling per concept, with a replaced form
  moved to `avoid` with its reason. `decisions.md` prose can still hold an older rationale, but it prescribes nothing:
  the ruling wins.
- **Journal regrowth** (append-only notes swelling past 200 KB until nobody reads them): REDUCED. `decisions.md` holds
  distilled rulings edited in place, and a byte budget that only shrinks warns when one grows (§ Tooling).

Not fixed by any shape: prose rationale going stale, and evidence citations that point outside the repo (a Microsoft TBX
id, a macOS bundle path), which stay unverifiable.

Considered and not taken: a per-term `governs:` list of the keys each ruling covers, with the guide markdown generated
from the rows. The concept's English `match` finds those keys instead (so a new key is covered the day it lands, with no
list to maintain), and the brief replaces a rendered guide. Also considered: fixing only the citation format (key,
English, and translation in one checked slot). It would detect value drift but still store a value to maintain, and
leaves self-contradiction possible.

## Layout

Shared, language-agnostic:

- `docs/i18n/concepts.json`: the concept registry. One entry per recurring English concept, keyed by a kebab-case
  concept ID. Written once for all languages.
- `docs/i18n/translator-instructions.md` and `docs/i18n/translation-principles.md`: the standing instructions and the
  shared judgment calls (voice, names vs prose, no hedged grammar, native typography, escalation). Every brief embeds
  both, in that order.
- `docs/i18n/source-queue.md`: English-side problems translators log (a weak description, ambiguous English, a rule
  every language needs). The lead clears it with David and deletes each entry once resolved.

Per full-translation locale, in `docs/i18n/<tag>/`:

- `style.md`: the style guide, opening with a `## Digest` section right after the H1 intro: the must-know rules in
  ~1,500–2,000 tokens (formality and address, voice, capitalization, punctuation, quote and ellipsis conventions, brand
  inflection, plural notes, the top traps). The digest is the summary and the rest of `style.md` the elaboration. The
  brief copies the digest verbatim, up to the next `##` heading.
- `terms.json`: this locale's rulings, keyed by concept ID. Every ruling current and resolved: no duplicates, no
  superseded forms (a superseded form becomes an `avoid` entry with its reason).
- `decisions.md`: distilled rulings ("X over Y because Z", at most ~3 lines each, edited or replaced in place, the
  superseded ones deleted). Not read by default: the brief pulls the sections whose heading cites a batch key (§
  `decisions.md` headings). Held to a byte budget (§ Tooling).
- `mechanics.json`: the locale's typography, checked against its catalog (§ `<tag>/mechanics.json` schema). Optional
  until the locale declares it.
- `review-queue.md`: open flags for a future native reviewer. Not translator input.

Regional overlays (`en-GB`, `en-AU`) aren't part of this: they fork a handful of keys, and their `glossary.md` is the
fork table `apps/desktop/scripts/i18n-en-overlays.test.ts` mirrors.

## `concepts.json` schema

```json
{
  "operation": {
    "en": "operation",
    "match": ["operation", "operations"],
    "sense": "The category of file work Cmdr performs: copy, move, delete, trash, rename, folder/file creation, archive edit.",
    "notMatch": ["math operation"],
    "distinct": ["transfer"],
    "note": "optional, one or two sentences, language-agnostic boundary guidance"
  }
}
```

- `en` (required): the headword as it reads in the English UI.
- `match` (required, non-empty): lowercase English surface forms, matched case-insensitively at word boundaries against
  the VISIBLE English copy (placeholder names, plural/select categories, tag names, markdown code spans, and link
  targets are stripped first, so `{count, plural, …}` never reads as "count"). A multi-word phrase is fine
  (`go to path`). A trailing `*` means prefix (`index*`). A leading `=` means the WHOLE value, ignoring edge punctuation
  and case (`=back` hits `Back` and `Back…`, never "come back"): use it for a short UI label whose bare word also runs
  through prose. Keep them tight: they drive both the brief and the drift check.
- `notMatch` (optional): patterns in the same syntax as `match`. A key whose English hits one doesn't count for this
  concept, in the brief and the drift check alike. This is where an ENGLISH-sense exclusion goes ("come back" isn't the
  Back button, "apps like TextEdit" isn't the Like action), written once for every locale. A locale's `exceptions` are
  only for deviations specific to that language.
- `sense` (required): one sentence, what this concept means in Cmdr. Split one English word into several concepts when
  it has several senses (`browse-file-picker` vs `browse-archive`), give each a `match` as specific as possible (they
  may overlap; the brief shows both), and link them with `distinct`.
- `distinct` (optional): concept IDs a translator could confuse with this one. The brief pulls these in too, even when
  no batch key hits them.
- `note` (optional).
- No other fields: an unknown field is a schema error (it's almost always a typo that would silently drop data).

## `<tag>/terms.json` schema

```json
{
  "operation": {
    "chosen": "bewerking",
    "accept": ["bewerkingen"],
    "proseAccept": ["klus"],
    "forms": "pl. bewerkingen; compounds take -en- (Bewerkingenwachtrij, Bewerkingenlogboek)",
    "avoid": [{ "form": "actie", "why": "reads as a user action, not a file operation" }],
    "confidence": "high",
    "sources": "macOS Finder/AppKit; MS terminology; Double Commander",
    "note": "optional, max two sentences, locale-specific boundary",
    "exceptions": { "queue.empty.body": "names the three transfer kinds, not operations" },
    "decision": "Operation queue: de hernoeming van Overdrachtswachtrij"
  }
}
```

- `chosen` (required): the canonical form.
- `accept` (optional): other surface forms that count as this term (inflections, button vs verb form, case forms). The
  drift check treats `chosen` + `accept` as case-insensitive substrings of the visible translation, so a compound
  (`Bewerkingenwachtrij`) or an inflected brand (`Cmdrben`) counts, and a bare stem already covers its endings. A form
  ending in `*` is stricter: it must START a word (`wachtrij*` accepts `Wachtrijen`, not `Bewerkingenwachtrij`).
- `proseAccept` (optional): everyday synonyms accepted in PROSE keys only, never in a name key (§ Name keys). This is
  how the principles' names-vs-prose freedom is written down: the drift check counts these forms for a prose key and
  still locks every name to `chosen` / `accept`. The brief prints them as "prose only". Keep the list short: the ruling
  stays the default.
- `forms` (optional): free-text usage note (button vs verb vs title form, plural, compounding).
- `avoid` (optional): forms not to use, each `{ form, why }`. Superseded rulings go here.
- `confidence` (required): `confirmed` (a human signed off) | `high` (authoritative sources agree) | `tentative`
  (sources conflict or none had it).
- `sources` (required): short evidence string.
- `note` (optional).
- `exceptions` (optional): catalog key → reason, for keys whose English matches the concept but whose translation
  legitimately doesn't use `chosen` / `accept` IN THIS LANGUAGE. A key that's simply another English sense belongs in
  the concept's `notMatch` instead, once for all locales. Every key must exist in the English catalog.
- `decision` (optional): a heading of a `##` or `###` section in this locale's `decisions.md` (without the `#` marks),
  or a list of them. Each is the exact heading or a prefix only one heading starts with (`"Operation queue"` for
  `Operation queue: de hernoeming (…, 2026-08-08)`), so a heading can gain a date or a cited key without breaking the
  pointer. A prefix that fits several headings is an error: lengthen it.
- A concept this locale deliberately keeps English still gets an entry, with `chosen` = the English form.
- A locale may omit concepts it has no ruling for; the brief then says "no ruling" for it.

### Name keys

A ruling locks NAMES (menu items, settings, commands, buttons, titles: what a user looks up and meets again elsewhere)
and allows `proseAccept` in prose. `isNameKey` in `apps/desktop/scripts/i18n-termbase-lib.ts` decides, from the key and
its visible English, first rule wins:

1. `menu.*` is a name: it's the native menu.
2. A sentence is prose: the visible English ends in `.` `!` `?` (closing quotes and brackets allowed after it), or holds
   a sentence break (`. ` before a capital).
3. A fragment of at most four words is a name, wherever it sits: a tooltip reading "Close" names its button.
4. A longer fragment is a name when the key's last segment is label-typed (`label`, `title`, `name`, `button`,
   `heading`, `tab`, `badge`, `aria`, or a camelCase `…Label`, `…Title`, `…Aria`, and so on), and prose otherwise.

It leans to "name" on purpose. On the English catalog at the time of writing: about 2,300 names, 1,400 prose keys.

## `<tag>/mechanics.json` schema

A locale's typography, which the brief prints beside its digest and `i18n-mechanics` checks against its catalog:

```json
{
  "quotes": { "primary": ["«", "»"], "nested": ["“", "”"] },
  "ellipsis": { "glyph": "…" },
  "apostrophes": ["’"],
  "spacing": [{ "pattern": "«(?![\\u00a0\\u202f])", "why": "a no-break space follows «" }],
  "hedges": [{ "pattern": "\\p{L}\\((?:e|s)\\)", "why": "a bracketed ending; use ICU select or plural, or rephrase" }]
}
```

- `quotes.primary` (required), `quotes.nested` (optional): each an opening and a closing mark, from the known quotation
  marks (`QUOTE_MARKS` in `apps/desktop/scripts/i18n-mechanics-lib.ts`). The straight `"` is never allowed.
- `ellipsis` (required): `glyph` is the ellipsis as the language's macOS writes it (`…`, or `⋯` in zh-Hant; the set is
  `ELLIPSIS_GLYPHS`), and the other glyph and `...` are findings. `spaceBefore` (optional) is the no-break space (U+00A0
  or U+202F) a LABEL-ENDING ellipsis takes (German `Wird geladen …`); without it, such an ellipsis hugs its word. A
  label-ending ellipsis follows a word and ends the text or meets punctuation; a leading (`…und mehr`), range (`1…5`),
  or truncating (`Datei…name`) one is never checked for spacing.
- `apostrophes` (optional): the characters this language writes as an apostrophe (`’`, or `'` where the straight one is
  the norm). Without it, any `'` or `’` outside the quote pairs is a finding.
- `spacing` (optional): each required spacing rule, written as the pattern that BREAKS it (a JavaScript regex, `u`
  flag), with a one-line `why`. A placeholder reads as U+FFFC in the scanned text, so a rule can require a space next to
  an insert.
- `hedges` (optional): the parenthesized or slashed alternatives this language's grammar tempts translators into
  (principles § No hedged grammar), same shape.
- `$comment` (optional). No other fields.
- `‹` `›` may be declared but are never a finding undeclared: they're also the breadcrumb separator ("Settings › AI").
- A missing file means "not declared yet", and the check skips the locale; an overlay without its own file inherits its
  base language's. `_template/mechanics.json` is a stub whose empty pairs fail the schema until filled (drop `nested`
  when the language has no nested marks).

## `decisions.md` headings

The brief finds a section by the keys its heading cites, so cite every key the section is about, in backticks:

- A full key: `` `servers.sheet.remember` ``.
- A wildcard: `` `servers.*` ``, `` `errors.mutation.trash*` ``, or one mid-key (`` `mtp.*.retry` ``).
- A sibling shorthand after a full key, with or without the leading dot:
  `` `settings.analytics.enabled.label`/`.description` `` resolves to `settings.analytics.enabled.description`.
- A citation that doesn't start at a catalog namespace (`` `rollbackConfirm.body` ``) matches as a segment-aligned
  suffix of a key. One that does start at a namespace is anchored there, so `servers.*` never claims
  `settings.servers.*`.

A `###` section whose `##` parent also matches is left out, since the parent's body already holds it.

## Tooling

### `pnpm i18n:brief` (`apps/desktop/scripts/i18n-brief.ts`, assembly in `i18n-brief-lib.ts`, memory in `i18n-brief-memory.ts`)

- `--lang nl` | `--lang nl,de` | `--lang all` (every full translation; overlays are refused).
- Key selectors, which narrow each other when combined: `--keys a.b.c,servers.sheet.*` (exact keys or `*` globs; a
  pattern matching nothing is an error), `--keys-file <path>` (the same, one per line, `#` comments allowed),
  `--missing` (English keys absent from any target locale), `--changed-since <git-ref>` (English keys added or whose
  value changed since the ref).
- `--out <file>` (default stdout), `--stats` (chars and rough tokens per section, to stderr).
- `--exclude-target-values`: a blind run for A/B tests. Withholds the batch keys' current translations and the decision
  excerpts (a section citing a batch key discusses exactly those translations), drops a ruling's batch-key `exceptions`
  and its `decision` pointer, and redacts the batch's shipped values, whole or clause by clause (10+ chars), wherever
  the digest or a ruling quotes them. Best-effort: a doc that paraphrases a value still gets through. The translation
  memory never contains batch keys anyway.
- `--messages-root <dir>` / `--docs-root <dir>`: fixtures.

Sections, each selected by the batch:

1. Header: languages, how the keys were chosen (a list over three patterns is summarized: the keys section spells it
   out), the absolute reference-pile path in the MAIN clone (`<main clone>/_ignored/i18n/<tag>/`, resolved through
   `git rev-parse --git-common-dir`, so it's right from a worktree), and for a single language, the full `style.md`.
2. Instructions: everything under `## Instructions` in `docs/i18n/translator-instructions.md`, with `{{LANGUAGE}}`
   ("Dutch (nl)") and `{{TAG}}` (`<tag>` for several languages) filled in, once. That file is the only home of the
   translator's standing instructions, so the guide and every brief can't drift apart.
3. Principles: everything under `## Principles` in `docs/i18n/translation-principles.md`, the same way, once.
4. Each language's `## Digest`, or a pointer to `style.md` when it has none yet, then one **Mechanics** line from its
   `mechanics.json` (quote pairs, ellipsis, apostrophes, spacing and hedge patterns), or "no mechanics.json yet".
5. Keys: key, English value, `@key` description, placeholders (described ones from `@key.placeholders`, the rest bare)
   and tags, each target's current value, and "No concept yet": the key's content words no concept's `match` covers,
   leaving out generic English, brand words, and words fewer than three English keys use (a concept recurs). That line
   is how a missing concept ("offline") shows up as work instead of passing for settled.
6. Terms in play: every concept whose `match` hits (and `notMatch` doesn't) a batch key's English. The sense, note, and
   hit keys print once; then one line per language with chosen / accept / prose only / forms / avoid / note /
   confidence, the `exceptions` for batch keys, and the `decision` heading, or "no ruling". Then up to eight `distinct`
   neighbors no key hits, one line each (sense plus each language's chosen form), and only those whose `match` head word
   appears somewhere in the batch's English: a neighbor the batch never mentions can't be confused with anything in it.
7. Translation memory: per key, the nearest SHIPPED keys (at least one target has them), never batch keys. Same-parent
   siblings take up to half the slots (they share a dialog, so they share its voice), ranked by word overlap then
   catalog distance. The rest must share two content words with the key, or be a short label whose every word the key
   contains ("Overwrite"): one shared word ("screen" in "Exit full screen" vs "Error screen") is noise, and an unfilled
   slot stays empty. Four neighbors for one language, three for several; a neighbor already listed under an earlier key
   is referenced as "(above)".
8. Decision excerpts per language: the sections citing a batch key, narrowest citation first, capped (1,500 chars and
   six sections for one language, 700 chars and three for several) with a `file:line` pointer to the full section; the
   overflow is listed by heading.

Deterministic (no time, RNG, or model), about 0.3 s for a 40-key batch.

### `pnpm i18n:check-termbase` (`apps/desktop/scripts/i18n-check-termbase.ts`, Go check `desktop-i18n-termbase`)

- **Schema problems are an ERROR** (exit 3): an unknown concept ID in `terms.json`, a missing `chosen` / `confidence` /
  `sources`, a bad confidence, an `exceptions` key the English catalog lacks, a `decision` naming no heading, a
  malformed `avoid`, a non-kebab concept ID, an empty or uppercase `match` form, a dangling `distinct`, unknown fields,
  and a STALE exception: an `exceptions` key whose English no longer matches the concept (after a `match` / `notMatch`
  change or an English edit). It excuses nothing today and would silently excuse the key again later, so drop it.
- All matching reads curly apostrophes and quotes (’ ‘ “ ”) as straight ones, in the forms and in the copy, English and
  translated alike.
- **Coverage drift is a WARN** (exit 1): for each ruling, every shipped key whose English matches the concept while its
  translation contains none of `chosen` / `accept` (plus `proseAccept` when the key is prose, § Name keys) and isn't in
  `exceptions`. Held to a per-locale count baseline in `apps/desktop/scripts/i18n-termbase-baseline.json` (`drift`): at
  or under it, one line; past it, every drifting key. Local runs ratchet the number down and drop a locale that lost its
  `terms.json`; a locale not listed is strict from its first `terms.json`. Same design as
  `i18n-check-term-consistency.ts`'s `notYetReviewed`.
- **`decisions.md` growth is a WARN** (exit 1): the same baseline file records each locale's `decisions.md` byte size
  (`decisionsBytes`), and a file past its number warns. Local runs ratchet it down to the current size, record a
  locale's first `decisions.md` at its size, and drop one that's gone. So the file only grows through a deliberate bump
  (David's OK), and distilling it (editing an entry in place, deleting what's superseded) lowers the budget for good.
- A locale without `terms.json` is skipped silently.
- `--list` prints every drifting key even under the baseline: the cleanup view.
- A `<tag>/concepts-proposed.json`, when a parallel fan-out leaves one, is validated too, and its IDs count as known for
  that locale's `terms.json` until they're merged.

### `pnpm i18n:check-mechanics` (`apps/desktop/scripts/i18n-check-mechanics.ts`, Go check `desktop-i18n-mechanics`)

- **Schema problems are an ERROR** (exit 3): a malformed `mechanics.json` (§ `<tag>/mechanics.json` schema). That locale
  isn't scanned until it's fixed.
- **Typography findings are a WARN** (exit 1), one per key and rule, in catalog VALUES: a straight `"` (every declared
  locale), a quotation mark outside the declared pairs and apostrophes, an ellipsis off the declared `ellipsis` (`...`,
  the other glyph, or the wrong space before a label-ending one), and a hit of a `spacing` or `hedges` pattern.
- It reads what the reader sees: ICU values through the runtime's parser (a doubled `''` is one apostrophe; a
  placeholder, `#`, or plural/select category is never text; each branch is scanned on its own), a raw family
  (`errors.*`, `menu.*`) literally with each `{token}` an insert. Markdown code spans (a command typed verbatim) and
  link targets are skipped.
- Held to a per-locale count baseline in `apps/desktop/scripts/i18n-mechanics-baseline.json`, the same ratchet-down
  design as termbase drift. A locale not listed is strict; `node scripts/i18n-check-mechanics.ts --adopt <tag>` records
  a newly declared locale at its current count, once (it refuses a locale that already has a number), so a locale can
  declare its rules first and clean up after.
- A locale without `mechanics.json` is skipped (not declared yet); an overlay without one inherits its base language's.
- `--list` prints every finding even under the baseline.

### `desktop-i18n-doc-citations`

Scans `concepts.json`, `<tag>/concepts-proposed.json`, and `<tag>/terms.json` alongside the markdown, so a backticked
key in a note or an `exceptions` reason must be a real key. Mechanism: `scripts/check/checks/DETAILS.md` § "The
doc-citation check".

## Writing back during translation

- New concept: add it to `concepts.json`. When several agents work in parallel (a fan-out across locales), each adds its
  new concepts to `<tag>/concepts-proposed.json` instead (same schema), and one agent merges them into `concepts.json`
  afterwards, so they don't collide on the shared file.
- New or changed ruling: edit the entry in place in `terms.json`; a replaced form moves to `avoid` with its reason.
- A key whose English uses the concept but whose translation rightly doesn't use the ruling: add it to that term's
  `exceptions` with the reason.
- An everyday synonym that reads better in running prose (never in a name): the ruling's `proseAccept`.
- Rationale worth keeping beyond one line: a `decisions.md` section whose heading cites the keys in backticks, linked
  from the term's `decision`. Distilled: "X over Y because Z", at most ~3 lines. A changed ruling edits or replaces its
  entry and deletes what it supersedes; never a narrative, a dated story, or a second entry beside the first.
- Anything that applies beyond one language (a rule, a weak description, ambiguous English): `source-queue.md`, never
  the locale's files.
- A typography rule the language needs (a quote pair, a spacing rule, a hedge its grammar invites): `mechanics.json`.
- Things only a native reviewer can settle: `review-queue.md`.
