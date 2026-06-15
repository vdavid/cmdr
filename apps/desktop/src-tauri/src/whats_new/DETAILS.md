# What's new parser details

Depth and rationale. `CLAUDE.md` holds the must-knows; this is the exact parse contract.

## What the parser captures

- Recognizes `## [x.y.z] - YYYY-MM-DD` release headings top-down. Skips the top `## [Unreleased]` block (no date, not a
  release) and ends the current release on any other H2.
- Captures the **lead** (prose paragraphs between the heading and the first `###`, blank lines preserved as paragraph
  breaks) and the Added / Changed / Fixed / Security sections, in changelog order. Drops `Non-app` and any unknown
  section name.
- Omits a release that has no lead AND no displayable section.

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
