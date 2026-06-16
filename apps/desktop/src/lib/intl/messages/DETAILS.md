# Message catalog details

Depth behind [`CLAUDE.md`](CLAUDE.md). How catalogs are authored, the `@key` metadata schema, and the parity contract.
The runtime that consumes these files (resolution, fallback, ICU, the error-pipeline boundary, the formatting split):
[`../DETAILS.md`](../DETAILS.md).

## Why per-area-central files

JSON, per feature area, under `en/`, plus `common.json` for genuinely shared strings. This is the chosen middle between
one giant file and fully-colocated fragments: clean diffs, an agent editing one feature touches one file, and a future
translate-a-locale job globs ~12 predictable files. Key prefix ↔ filename is 1:1, so the area in a key
(`settings.fsWatch.title`) IS its catalog home (`settings.json`) AND its scope. The same English word can diverge per
window just by getting its own area key; shared strings live in `common.*`, and the moment one site needs different copy
it gets its own area key (never a positional "window" argument).

The catalog areas are a closed set, enforced by `desktop-message-key-naming`'s `messageKeyKnownAreas`, and they mirror
the `lib/` feature directories (lowerCamel): `common`, `transfer`, `settings`, `errors`, `fileExplorer`,
`fileOperations`, `queryUi`, `search`, `viewer`, `onboarding`, `licensing`, `downloads`, `ai`, `goToPath`, `mtp`, `ui`,
`updates`, `whatsNew`, `commandPalette`, `commands`, `shortcuts`, `crashReporter`, `errorReporter`, `feedback`, `menu`,
`indexing`, `lowDiskSpace`, `notifications`, `main`. Adding one means adding both the `<area>.json` file and the area to
that check.

## Message value format

A value is either a plain ICU string or, for plurals/selects, an ICU string with `{count, plural, …}` /
`{type, select, …}` inline (NOT a `{one, other}` object — the engine parses the inline form). Examples live in
`en/transfer.json` (the hardest multi-variable case: a `kind` discriminator `select` wrapping independent `plural`
branches, with preformatted `*Text` count strings for display and raw integers for plural selection).

Rich-text (inline-component) messages use `<tag>…</tag>` markers that `<Trans>` maps to Svelte snippets, e.g.
`en/common.json`'s `common.downloadsFdaHint`: `"… <settingsLink>Open System Settings</settingsLink>"`. No `{@html}`;
text is text, components are components.

## ICU apostrophe rule

ICU MessageFormat treats `'` as an escape character. A lone `'` is literal UNLESS it immediately precedes a special char
(`{`, `<`, `#`), where it opens a quoted section that swallows following text until the next `'`. `''` always collapses
to a single `'`. (Verified on `intl-messageformat@11.2.7`, by reading the parser behavior, 2026-06-16.)

Cmdr copy is full of apostrophes ("doesn't", "can't", "you're", "already at the target"). The rule is to double EVERY
apostrophe in a catalog value, not only the dangerous ones: `''` is always safe, and a blanket rule survives future copy
edits that might move an apostrophe next to a placeholder. The per-area parity test is the net that catches a missed
double.

## `@key` metadata schema

Each message key MAY have a sibling `@`-prefixed entry holding ARB-style metadata, stripped before the runtime or
codegen ever sees it:

```jsonc
{
  "transfer.trash": "Moved {countText} {count, plural, one {file} other {files}} to trash",
  "@transfer.trash": {
    "description": "Toast confirming items were moved to the macOS Trash. Shown briefly after a delete-to-trash (F8).",
    "placeholders": {
      "countText": "how many files, already formatted for display (e.g. 1,234)",
      "count": "the same number of files (drives the singular/plural form of the noun)",
    },
    "screenshot": "transfer-complete-toast.png",
  },
}
```

- `description`: a free-form, translator-facing note. Optimize it to set a translator (human or agent) up to do
  EXCELLENT work without seeing the running app. Cover what's string-SPECIFIC: (1) where and when it appears (the
  surface and the trigger — "status-bar toast after a copy", "label in the Settings > Appearance section", "button in
  the delete-confirm dialog"); (2) the tone or intent if it's not obvious (reassuring, a warning, a terse control
  label); (3) any constraint that shapes the translation (a tight button/column that can't grow much, a term that must
  match a sibling string, a literal token that must NOT be translated — brand names like "Finder"/"GitHub", format
  tokens like `YYYY`/`MM`, shortcut glyphs). Keep it natural language, not a rigid schema. Do NOT explain the ICU
  plumbing (the translator already knows to preserve placeholder/`plural`/`select` syntax and apply their language's
  plural categories — that's a one-time instruction in the translator-agent prompt, not per-string noise).
- `placeholders`: an ARB-style map giving each placeholder a PLAIN-LANGUAGE meaning plus an example value, in the
  translator's terms ("number of files", "the folder name") — never the ICU mechanics ("raw integer for plural
  selection"). Include it whenever a message has placeholders; omit it for static strings. This is what lets a
  translator reorder placeholders correctly for their grammar.
- `screenshot`: a filename in `screenshots/` showing the string in context. One screenshot may serve many keys; many
  keys can name the same file. Screenshots are optional during migration, and a screenshot REPLACES the need to describe
  layout in prose.

The guiding test for every `@key`: "Could a competent translator who has never run Cmdr render this perfectly into any
language, given this note plus a per-language style guide plus (optionally) the screenshot?" If not, the note is missing
context, a placeholder meaning, or a constraint. Tone/voice/formality are NOT per-string — they live in the per-language
style guide, so don't repeat them on every key.

Keep the `@key` twin's name in lockstep with its message key on a rename. `desktop-message-key-naming` validates the
twin too (it strips the leading `@` and checks the underlying key), so a metadata entry for a misnamed key also fails.

## Parity contract

`en` is the base/source locale. Every migrated string's base-locale rendered output must equal the pre-migration English
(this is readiness, not a copy change). The per-area parity test asserts this (the transfer pilot's is
`../../file-operations/transfer/transfer-complete-toast.test.ts`, pinned to en-US). The `pluralize-noun` Go check scans
source, not catalog JSON, so ICU plurals inside catalogs aren't covered by it — their correctness is the parity test's
job.

## Dead-key honesty

The codegen's dead-key warning lists catalog keys never referenced in code. The usage scan only sees STATICALLY-written
keys (`t('literal')`, `<Trans key="literal">`), so a dynamically-built key reads as dead. Verify a key is truly unused
before deleting it on this warning alone. `common.downloadsFdaHint` is a current known dead key: it's the M0 `<Trans>`
proof, with a real call site (the Downloads FDA hint) coming in an M2 tranche.

## Principle 6 note (humans review human-facing copy)

The base `en` catalog is a parity-protected MOVE of already-human-authored copy, so it's fine under principle 6. But
future agent-translated locales DO meet human eyes, so that later pipeline must include human review — "agents
translate, scripted pipeline" is not a license to ship unreviewed machine copy.
