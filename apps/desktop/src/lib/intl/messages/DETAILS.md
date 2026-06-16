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

The catalog areas are a closed set, enforced by `desktop-message-key-naming`'s `messageKeyKnownAreas`. The planned full
set (Decision 4): `common`, `transfer`, `settings`, `errors`, `search`, `viewer`, `menu`, `commands`, `onboarding`.
Adding one means adding both the `<area>.json` file and the area to that check.

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
    "description": "Toast after moving items to the system trash. {countText} preformatted count; {count} raw integer for plural selection.",
    "screenshot": "transfer-complete-toast.png",
  },
}
```

- `description`: a translator-facing note — what the string is, what each placeholder means, any agreement/plural
  nuance.
- `screenshot`: a filename in `screenshots/` showing the string in context. One screenshot may serve many keys; many
  keys can name the same file. Screenshots are optional during migration.

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
