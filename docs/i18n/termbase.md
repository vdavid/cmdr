# Termbase: schema and tooling

Why: per-locale glossaries grew to 210–375 KB of mostly append-only journal, so translators loaded all of it or grepped
and missed rulings. A structured termbase lets `pnpm i18n:brief` hand a translator only what one batch needs.

The process that uses all this (who translates, the agent prompt): `docs/guides/i18n-translation.md`.

## Layout

Shared, language-agnostic:

- `docs/i18n/concepts.json`: the concept registry. One entry per recurring English concept, keyed by a kebab-case
  concept ID. Written once for all languages.

Per full-translation locale, in `docs/i18n/<tag>/`:

- `style.md`: the style guide, opening with a `## Digest` section right after the H1 intro: the must-know rules in
  ~1,500–2,000 tokens (formality and address, voice, capitalization, punctuation, quote and ellipsis conventions, brand
  inflection, plural notes, the top traps). The digest is the summary and the rest of `style.md` the elaboration. The
  brief copies the digest verbatim, up to the next `##` heading.
- `terms.json`: this locale's rulings, keyed by concept ID. Every ruling current and resolved: no duplicates, no
  superseded forms (a superseded form becomes an `avoid` entry with its reason).
- `decisions.md`: the rationale journal. Not read by default: the brief pulls the sections whose heading cites a batch
  key (§ `decisions.md` headings).
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
  (`Bewerkingenwachtrij`) or an inflected brand (`Cmdrben`) counts.
- `forms` (optional): free-text usage note (button vs verb vs title form, plural, compounding).
- `avoid` (optional): forms not to use, each `{ form, why }`. Superseded rulings go here.
- `confidence` (required): `confirmed` (a human signed off) | `high` (authoritative sources agree) | `tentative`
  (sources conflict or none had it).
- `sources` (required): short evidence string.
- `note` (optional).
- `exceptions` (optional): catalog key → reason, for keys whose English matches the concept but whose translation
  legitimately doesn't use `chosen` / `accept` IN THIS LANGUAGE. A key that's simply another English sense belongs in
  the concept's `notMatch` instead, once for all locales. Every key must exist in the English catalog.
- `decision` (optional): the exact heading text (without the `#` marks) of a `##` or `###` section in this locale's
  `decisions.md`.
- A concept this locale deliberately keeps English still gets an entry, with `chosen` = the English form.
- A locale may omit concepts it has no ruling for; the brief then says "no ruling" for it.

## `decisions.md` headings

The brief finds a section by the keys its heading cites, so cite every key the section is about, in backticks:

- A full key: `` `servers.sheet.remember` ``.
- A wildcard: `` `servers.*` `` or `` `errors.mutation.trash*` ``.
- A sibling shorthand after a full key, with or without the leading dot:
  `` `settings.analytics.enabled.label`/`.description` `` resolves to `settings.analytics.enabled.description`.
- A citation that doesn't start at a catalog namespace (`` `rollbackConfirm.body` ``) matches as a segment-aligned
  suffix of a key. One that does start at a namespace is anchored there, so `servers.*` never claims
  `settings.servers.*`.

A `###` section whose `##` parent also matches is left out, since the parent's body already holds it.

## Tooling

### `pnpm i18n:brief` (`apps/desktop/scripts/i18n-brief.ts`, assembly in `i18n-brief-lib.ts`)

- `--lang nl` | `--lang nl,de` | `--lang all` (every full translation; overlays are refused).
- Key selectors, which narrow each other when combined: `--keys a.b.c,servers.sheet.*` (exact keys or `*` globs; a
  pattern matching nothing is an error), `--missing` (English keys absent from any target locale),
  `--changed-since <git-ref>` (English keys added or whose value changed since the ref).
- `--out <file>` (default stdout), `--stats` (chars and rough tokens per section, to stderr).
- `--exclude-target-values`: a blind run for A/B tests. Withholds the batch keys' current translations AND the decision
  excerpts, since a section citing a batch key discusses exactly those translations. The translation memory never
  contains batch keys anyway.
- `--messages-root <dir>` / `--docs-root <dir>`: fixtures.

Sections, each selected by the batch:

1. Header: languages, how the keys were chosen, the absolute reference-pile path in the MAIN clone
   (`<main clone>/_ignored/i18n/<tag>/`, resolved through `git rev-parse --git-common-dir`, so it's right from a
   worktree), and for a single language, the path of the full `style.md`.
2. Each language's `## Digest`, or a pointer to `style.md` when it has none yet.
3. Keys: key, English value, `@key` description, placeholders (described ones from `@key.placeholders`, the rest bare)
   and tags, and each target's current value.
4. Terms in play: every concept whose `match` hits a batch key's English. The sense, note, and hit keys print once; then
   one line per language with chosen / accept / forms / avoid / note / confidence, the `exceptions` for batch keys, and
   the `decision` heading, or "no ruling". Their `distinct` neighbors that no batch key hits follow as one line each
   (sense plus each language's chosen form): enough to keep two senses apart at a fraction of the size.
5. Translation memory: per key, the nearest SHIPPED keys (at least one target has them), never batch keys. Same-parent
   siblings take up to half the slots (they share a dialog, so they share its voice), ranked by word overlap then
   catalog distance; the rest go to the highest IDF-weighted English word overlap across the catalog, so the same phrase
   on another surface shows up. Four neighbors for one language, three for several; a neighbor already listed under an
   earlier key is referenced as "(above)".
6. Decision excerpts per language: the sections citing a batch key, narrowest citation first, capped (1,500 chars and
   six sections for one language, 700 chars and three for several) with a `file:line` pointer to the full section; the
   overflow is listed by heading.
7. Footer: the write-back instructions below.

Deterministic (no time, RNG, or model), about 0.2 s for a 40-key batch.

### `pnpm i18n:check-termbase` (`apps/desktop/scripts/i18n-check-termbase.ts`, Go check `desktop-i18n-termbase`)

- **Schema problems are an ERROR** (exit 3): an unknown concept ID in `terms.json`, a missing `chosen` / `confidence` /
  `sources`, a bad confidence, an `exceptions` key the English catalog lacks, a `decision` naming no heading, a
  malformed `avoid`, a non-kebab concept ID, an empty or uppercase `match` form, a dangling `distinct`, unknown fields.
- **Coverage drift is a WARN** (exit 1): for each ruling, every shipped key whose English matches the concept while its
  translation contains none of `chosen` / `accept` and isn't in `exceptions`. Held to a per-locale count baseline in
  `apps/desktop/scripts/i18n-termbase-baseline.json`: at or under it, one line; past it, every drifting key. Local runs
  ratchet the number down and drop a locale that lost its `terms.json`; a locale not listed is strict from its first
  `terms.json`. Same design as `i18n-check-term-consistency.ts`'s `notYetReviewed`.
- A locale without `terms.json` is skipped silently.
- `--list` prints every drifting key even under the baseline: the cleanup view.
- `<tag>/concepts-proposed.json` is validated too, and its IDs count as known for that locale's `terms.json`.

### `desktop-i18n-doc-citations`

Scans `concepts.json`, `<tag>/concepts-proposed.json`, and `<tag>/terms.json` alongside the markdown, so a backticked
key in a note or an `exceptions` reason must be a real key. An allowlist entry keyed by `<tag>/glossary.md` follows its
citation to `<tag>/decisions.md` when a locale migrates. Mechanism: `scripts/check/checks/DETAILS.md` § "The
doc-citation check".

## Writing back during translation

- New concept: add it to `concepts.json`. During the parallel per-locale migration only, add it to
  `<tag>/concepts-proposed.json` instead (same schema, merged into `concepts.json` afterwards), so parallel agents don't
  collide on the shared file.
- New or changed ruling: edit the entry in place in `terms.json`; a replaced form moves to `avoid` with its reason.
- A key whose English uses the concept but whose translation rightly doesn't use the ruling: add it to that term's
  `exceptions` with the reason.
- Rationale worth keeping beyond one line: a `decisions.md` section whose heading cites the keys in backticks; link it
  from the term's `decision`.
- Things only a native reviewer can settle: `review-queue.md`.
