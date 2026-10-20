# Redact

Turns log and report text into shape-preserving tokens. Our own log sites MARK every user value
(`cmdr_fs::log_private`), so the span pass hashes exact values by the producer's kind. One composed regex is the
fallback for text nobody could mark: OS, server, and CLI output, third-party crates, lines from older builds. ONE
policy: unsalted `redact_line` (MCP resources, the crash hook) writes bare tokens (`<dir>`),
`RedactionContext::redact_line` (reports) writes tokens that correlate only within one report and process. Typed values
(`redact_path`, `redact_name`, …) fail closed. Pattern table and decisions: `DETAILS.md`.

## Must-knows

- **Spans go first** (`markers.rs`), at any `{:?}` depth: depth is `1 + trailing_zeros` of the backslash run, and a span
  closes only at its own depth, so a hostile name can't close it early. A broken span fails closed (to the line's or
  field's end; a stray closer back to the last boundary). ❗ The regex sees a placeholder per span, ❌ never the token.
  `DETAILS.md` § "Marked spans".
- **A leak from our own log site is fixed at the site**, ❌ not with a new regex shape: `LogPrivate::<kind>` (keeping
  `key=` and the format spec), `LogError(&e)` for a path-carrying error, `LogDetail(text).marking(&[…])` for external
  text in `detail=` / `stderr=` / `stdout=`, `cmdr_fs::log_json` for a value logged as its JSON. The `log-private-values` check fails on any unmarked value. A crate without
  `cmdr-fs` logs `key={:?}` under a `path_field` key, or `host=` / `server=` / `share=` / `user=`. ❌ Never redact at
  the log site.
- **❌ Never branch the policy on `context.is_none()`.** A second MCP policy once let a URL swallow a following path
  (cmdr-reports#30); `unsalted_api_runs_the_report_policy_with_bare_tokens` pins it.
- **The fallback keeps shape, never identity**: a fixed mount/home prefix, an allowlisted parent, a conservative
  extension (an extensionless leaf reads `<dir>` unless a span said file); a remote reference redacts as one unit;
  only exact derived-ID shapes are recognized.
- **A prose path runs to the next hard delimiter** (`split_trailing_noise`): the `: ` seam, a new reference, ` -> `,
  and `, ` / `; ` / ` key=` after the last separator. ❌ Never end a path on a word's shape (a `)`, a dot, a lowercase
  run): each such rule shipped part of a name. `DETAILS.md` § "Finding the end of a path".
- **Tokens key on a domain plus the normalized name** (escapes undone, NFC, Cmdr temp suffix split off). Typed inputs
  never trust token-looking syntax.
- **`redact_with` resumes at `match.start() + consumed`, ❌ never `replace_all`.** A handed-back tail must face the
  scanner again.

## Gotchas

- **A bare name goes only when its line also names it** in a path, a span, or a key (the leaf echo scrub, never the
  log header). Pinned by `a_path_leaf_repeated_bare_on_its_line_is_scrubbed`.
- **Any absolute path is a path** (`abs_path`, `\B/` plus two segments); OS junk names (`.DS_Store`, …) ship verbatim.
- **Dispatch order mirrors regex alternation order.** Complete URLs before generic userinfo, derived IDs before IPs.

## Files

`mod.rs` (API, regex, dispatch), `markers.rs` (the span pass, and the sinks' `display_line`), `context.rs`, `paths.rs`,
`references.rs`, `fields.rs` (keyed fields), `detail.rs` (external text, echo scrub), `path_end.rs`, `names.rs`, leaves
`tokens.rs`, `placeholders.rs`, `echoes.rs`; tests in `*tests.rs` (`bundle_leak_tests.rs`: shapes that shipped in real
bundles), golden corpus in `fixtures/`.
