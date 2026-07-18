# What's new parser details

Depth and rationale. `CLAUDE.md` holds the must-knows; this is the exact parse contract.

## What the parser captures

- Recognizes `## [x.y.z] - YYYY-MM-DD` release headings top-down. Skips the top `## [Unreleased]` block (no date, not a
  release) and ends the current release on any other H2.
- Captures the **lead** (the block between the heading and the first `###`) and the Added / Changed / Fixed / Security
  sections, in changelog order. Drops `Non-app` and any unknown section name.
- Omits a release that has no lead AND no displayable section.

### Lead line-joining (so a lead can carry a wrapped numbered list)

`build_lead` separates blank-line-delimited paragraphs with `\n\n`. *Within* a paragraph it joins each line that starts
a Markdown list item (`- ` / `* ` / `+ ` or `N.` / `N)`) onto a fresh line, and every other line onto the previous one
with a space (soft-wrap continuation). This is load-bearing for a lead shaped as a bold headline plus a real Markdown
numbered list (`1.` / `2.` / `3.`):

- **Markers must stay at line-start.** Both renderers (snarkdown in the app popup, marked on the website) only recognize
  a list marker at the **start of a line**, so a blanket space-join would flatten `1. … 2. … 3. …` into literal inline
  text instead of an `<ol>`.
- **Continuation lines must reflow onto their item.** snarkdown's list parser has no lazy-continuation: a highlight that
  the changelog formatter wraps at ~100 chars leaves a bare, un-indented continuation line, and snarkdown treats that
  line as **closing** the `<ol>`, printing it as loose text and making the next `N.` open a fresh list that restarts at
  1. Joining the continuation onto the item's line with a space keeps each item a single line, which both renderers
  render as one clean `<ol>`. (marked survives the un-reflowed form via lazy-continuation; snarkdown does not, hence the
  reflow.)

Prose is unaffected: a soft in-paragraph `\n` and a space both collapse to a space when either renderer emits HTML, so
reflowing wrapped prose to spaces reads identically. This is faithful rendering, not fix-up logic (the changelog stays
the single source of truth); the standard lead shape is a `**bold headline**`, optionally followed by a blank line and a
short numbered list of highlights.

## Per-entry post-processing

- Joins wrapped continuation lines.
- Strips the trailing `([hash](url), …)` commit group. Hashes are 6-8 hex chars and one entry may carry several
  comma-separated links wrapped across source lines, so the stripper matches the whole variable-length, multi-link
  trailing parenthetical structurally, only when every comma-separated item inside is a bare `[hex](url)` link. A real
  trailing aside like `(non-link aside)` survives.
- Flattens other markdown links to their label. Bold / italic / `code` / quotes stay verbatim.

## Version comparison

Uses the `semver` crate, so `0.9.0 < 0.10.0` (not string order).

## No `### Development history` cutoff

There's deliberately no special case for the `Development history` block. The slice never collects more than `max`
(≤ 5) releases, so the walk stops well before that block. Don't add a cutoff.
