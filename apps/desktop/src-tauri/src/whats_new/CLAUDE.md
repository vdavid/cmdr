# What's new parser

Parses the repo-root `CHANGELOG.md` into a typed, user-facing model for the post-update "What's new" popup. The whole
slice never exposes more than the five newest in-range releases; older notes live on the website.

## Module map

- `mod.rs`: types (`WhatsNewRelease`, `WhatsNewSection`), the top-down parser, entry post-processing (continuation
  joins, commit-link stripping, link flattening), `releases_between(since, current, max)` slicing, and a `OnceLock`
  cache over the embedded changelog.
- `tests.rs`: fixture-based unit tests plus one smoke test over the real embedded file.
- IPC lives in `../commands/whats_new.rs` (thin pass-throughs `get_whats_new` + `whats_new_dev_override`).

## The guardrail that matters most

**The changelog is the source of truth. Fix bad formatting THERE, never grow fix-up logic here.** Whatever lands in a
release's prose lead and its Added / Changed / Fixed / Security sections renders verbatim in the app. The parser only
strips machinery the user shouldn't see (the trailing commit-link group, the `Non-app` section, unknown sections) and
flattens non-commit markdown links to their text. It must NOT learn to "clean up" garbled entries. Garbage in the popup
gets fixed in `CHANGELOG.md`. Letting fix-up logic accrete here would make the parser the second source of truth and rot
the moment the two disagree.

**Resilience over strictness.** Malformed input must never panic or block startup: skip what doesn't parse, log at debug
(`target: "whats_new"`), show what does. `smoke_real_changelog_parses` is the canary if the changelog format drifts.

## Contract the parser applies

- Recognizes `## [x.y.z] - YYYY-MM-DD` release headings top-down. Skips the top `## [Unreleased]` block (no date, not a
  release) and ends the current release on any other H2.
- Captures the **lead** (prose paragraphs between the heading and the first `###`, blank lines preserved as paragraph
  breaks) and the Added / Changed / Fixed / Security sections, in changelog order. Drops `Non-app` and any unknown
  section name.
- A release with no lead AND no displayable section is omitted.
- Per entry: joins wrapped continuation lines, then strips the trailing `([hash](url), …)` commit group. Hashes are 6-8
  hex chars and a single entry may carry several comma-separated links wrapped across source lines, so the stripper
  matches the whole variable-length, multi-link trailing parenthetical structurally (only when every comma-separated
  item inside is a bare `[hex](url)` link, so a real trailing aside like `(non-link aside)` survives). Other markdown
  links flatten to their label; bold / italic / `code` / quotes stay verbatim.
- Version comparison uses the `semver` crate, so `0.9.0 < 0.10.0` (not string order).

There's no `### Development history` cutoff: the slice never collects more than `max` (≤ 5) releases, so the walk stops
well before that block. Don't special-case it.

## Gotchas

- **`include_str!` path depth.** The path runs up five levels from `src/whats_new/` to the repo root
  (`../../../../../CHANGELOG.md`). If files move it breaks at COMPILE time, which is the good failure mode.
- **Dev-mode staleness.** Editing `CHANGELOG.md` does NOT trigger a live `pnpm dev` rebuild (the Tauri watcher's
  `.taurignore` excludes `*.md`, and the changelog is embedded, not read at runtime). Cargo tracks the `include_str!`
  input, so the next real `cargo build` picks it up. Don't "fix" this by adding the file to the watcher: it would
  restart the app on every changelog edit.
- **No runtime I/O.** The changelog is embedded, so the commands can't hang and skip `blocking_with_timeout` on purpose.

Full details: [DETAILS.md](DETAILS.md).
