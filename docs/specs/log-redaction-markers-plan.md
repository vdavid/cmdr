# Log redaction markers plan

Issue: [#491](https://github.com/vdavid/cmdr/issues/491) (error-report bundles keep leaking file and folder names).
Evidence: `ERR-VEZUB` and `ERR-VUFQZ` (0.54.0), both already redacted; the leaks are what survived. Plan written
2026-10-19 against `ac4f12335` (#487 landed: every FE value renders through `describeForLog`). "Leak N" below means item
N of the issue's "What's wrong" list (1 paren in a segment, 2 FE-flattened typed error, 3 smb2 positional path, 4
third-party debug dumps, 5 unkeyed volume and share labels, 6 wrong kind tags, 7 over-redacted `scanned new dir`).

The idea in one line: mark every user-derived value where the line is written, so the bundle pass hashes exact spans
instead of guessing where a name ends. The regex redactor stays as the fallback for text nobody marked, with a greedy
boundary.

## Status and decisions for David

Status (2026-10-20): milestones 1–10 are done. What's left is #568 (error text flattened into a `String` before a log
macro sees it stays on the regex fallback). The lead made these calls; they're yours to confirm or overturn:

1. **Markers are Unicode noncharacters** (U+FDD0–U+FDEF), one opener per kind. The log file shows them as invisible
   chars (`<U+FDD2>` in `less`); printable brackets would collide with real names more often.
2. **Only our own crates' DEBUG/TRACE reaches a bundle**, decided at write time from `module_path()`; an untagged line
   counts as third-party. `mdns_sd` is capped at Info in the log file too (it was ~60% of a bundle).
3. **No clippy ban on `Path::display`** (~576 non-log calls). The `log-private-values` check guards log macros instead,
   strict with no allowlist, and `// allowed-log-private: <reason>` excuses a value that only looks like the user's.
4. **Errors mark their paths through a `LogError(&e)` wrapper**, never a marking `Display`, so markers can't reach UI
   text, IPC, or stored data. Cost: #568.
5. **The terminal, the debug log viewer, and MCP show a marked value unescaped** (`Café`); only the raw log file shows
   its `{:?}` spelling (`Cafe\u{301}`).

## Loud rules (every milestone)

- ❗ **Read first**: `apps/desktop/src-tauri/src/redact/CLAUDE.md` + `DETAILS.md` in full (the § "Finding the end of a
  path" history is WHY this effort exists), `apps/desktop/src-tauri/src/error_reporter/CLAUDE.md`,
  `apps/desktop/src-tauri/src/logging/CLAUDE.md`, `apps/desktop/src/lib/logging/CLAUDE.md` + `DETAILS.md`, and
  `docs/tooling/logging.md`. If a line ref here is off, trust the code and keep the intent.
- ❗ **Never ship a name.** Every ambiguity resolves toward redacting more. A change that makes any existing redaction
  cell ship more raw text is wrong, whatever else it fixes.
- ❗ **Markers can appear in real filenames.** Noncharacters are valid Unicode scalar values; APFS and ext4 store them.
  So escaping inside a wrapped value and fail-closed parsing are MANDATORY, not polish. A hostile name must never close
  a span early, and a broken span must never leave its tail unredacted.
- ❗ **Local logs stay readable and keep raw values.** Producers never redact. The terminal and the in-app debug log
  viewer strip markers; the log FILE keeps them (the bundle reads the file).
- ❗ **A marked string is log text, never data.** It must never become a filesystem path, a persisted value, or UI copy.
  Wrappers are for log formatting only.
- ❌ **One policy.** The span pass runs inside the shared dispatch, so unsalted `redact_line` (MCP `cmdr://logs`, the
  crash hook, `tools/quiet.rs`) and `RedactionContext::redact_line` behave the same, modulo token salt. ❌ Never branch
  on `context.is_none()`.
- ❌ **No string matching** to classify errors or state (`error-string-match`, `cmdr/no-error-string-match`). The span
  parser matches marker CHARACTERS, which is a lexical grammar, not error classification.
- ❌ **Don't edit `crates/cmdr-operations/src/transfer/`, `crates/cmdr-smb/`, `crates/cmdr-sftp/`, `cmdr-webdav/`,
  `cmdr-s3/`, `cmdr-adb/`, `crates/cmdr-fs/src/volume/`, `apps/desktop/src/lib/file-operations/transfer/`, or the `smb2`
  lines in any `Cargo.toml` / `Cargo.lock`** before the transfer-outages effort
  (`docs/specs/transfer-outages-and-cleanup-plan.md`, #489/#490) has landed. That effort rewrites `volume_outage.rs`,
  adds `VolumeError::transport_fault` in `cmdr-fs`, bumps smb2, and edits the transfer FE. Until it lands, this plan
  touches `cmdr-fs` only in a new `log_private.rs` (plus its `mod` line), `log_detail.rs`, and milestone 7c's log sites
  outside `src/volume/`. Milestones 8 and 9 come after it and take the rest.
- ❌ **Don't edit the smb2 or mtp-rs repos from here.** A separate agent acting for the crate's interests owns each
  (milestone 2).
- ❌ **No real user data in this public repo.** Test cells use synthetic names that reproduce the SHAPE of each leak
  (`Summer (Lake House) 2026`, `Alex's MacBook Pro (2)`, a share called `Family Photos`), never names from a bundle.
- **TDD**: every leak shape and hostile-name case gets a red cell first, seen failing for the right reason.
  Characterization pins reality.
- ❌ No milestone tags in code, comments, test names, `reason` strings, or docs
  (`rg -n '\b(M[0-9]+[a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b' <touched paths>` before reporting).
- **Docs move with the code**: `redact/CLAUDE.md` + `DETAILS.md`, `logging` docs on both sides,
  `docs/tooling/logging.md`. Describe current state, not history.
- Checks: `pnpm check --fast` per commit, `pnpm check svelte-tests-related` when FE files changed, `pnpm check clippy`
  and `pnpm check rust-tests` before reporting a Rust milestone (this is privacy work; don't leave the suite to CI).
  Foreground only.

## Census (fresh, `ac4f12335`)

A rough scan (multi-line-aware, non-test files) found 2,681 Rust log-macro invocations. About 980 carry `.display()` or
an argument or capture named like a path, name, dir, file, folder, share, volume, or label: an upper bound, since `name`
often names a setting. 389 call `.display()` inline; 216 already use a keyed `x={:?}` field. Per area (carriers):
`cmdr-index` 315, `cmdr-operations` 202 (127 in `transfer/`), `cmdr-smb` 64, desktop `network` 59, `file_system` 52,
`cmdr-mtp` 35, desktop `volumes` + `volumes_linux` 23, `commands` 15, `cmdr-local` 14, `search` 13, `cmdr-fs` 12,
`cmdr-s3` 12, `cmdr-listing` 11, `cmdr-webdav` 10, the long tail ~120. Outside log macros: 966 non-test `.display()`
calls in all, and 65 `format!(… .display() …)` building error strings. smb2 has ~310 log sites; its path and share
fields print bare (`path={}`, `share={}`).

Bundle composition (ERR-VEZUB, 13,946 lines): 8,669 are `mdns_sd::service_daemon` DEBUG. Dropping them frees most of the
bundle cap for Cmdr's own lines.

## The marker format (decided here, pending David's review)

- **One opener per kind, one closer, all from U+FDD0–U+FDEF.** Closer `U+FDD1`. Openers: path (kind unknown), path to a
  file, path to a dir, name (kind unknown), file name, dir name, volume label (shares the volume token domain with
  `share`), host, user. Exact code points are the executor's choice, defined once per language. A single-char opener
  keeps sinks trivial (strip a char range, nothing to parse) and keeps the `{:?}`-escaped form a fixed shape. This
  deviates from the issue's `U+FDD0 <kind> value U+FDD1` sketch on purpose.
- **The whole U+FDD0–U+FDEF range is reserved.** Sinks strip the whole range, raw AND in its escaped spellings (a
  backslash run + `u{fd[d-e][0-9a-f]}`): `LogDetail::marking` and any marked value that passes through a `{:?}` reach
  the file escaped, and the terminal and viewer must not show `\u{fdd2}…\u{fdd1}` litter.
- **Rust's `{:?}` escapes noncharacters** as `\u{fdd1}` (verified on rustc 1.99.0, a probe printing
  `format!("{:?}", "a\u{FDD1}b")`, 2026-10-19). So a marker that passes through any Debug formatting (a keyed `{:?}`
  field, `LogDetail`, a derived `Debug` on an error struct, a Debug of a Debug) arrives as text: `\u{fdd1}`,
  `\\u{fdd1}`, `\\\\u{fdd1}`. The parser accepts the raw char AND the escaped form preceded by ANY run of backslashes.
- **Depth comes from the run's trailing zeros, not its length.** A raw marker is depth 0. An escaped marker behind a
  MAXIMAL run of r ≥ 1 backslashes is depth `1 + r.trailing_zeros()`: at depth d the marker's own escape is 2^(d-1)
  backslashes, and every literal backslash in front of it arrives as a multiple of 2^d (a value's backslash is
  body-escaped once more than its span's depth, so 2^(d+1) each; foreign text's, 2^d each), so r = 2^(d-1) × odd.
  Counting the run's length instead would read `x\` + a depth-1 closer (r = 4 + 1 = 5) as a different depth, and every
  span whose value ends in a backslash would fail to close (fail-closed, but it eats the rest of the line).
- **One content encoding: a span's value is always written as the BODY of `{:?}`** (what Rust's `Debug` for `str` writes
  between its quotes), in `Display` and `Debug` alike, so a depth-d span holds its value escaped d+1 times and the
  parser unescapes it d+1 times (`redact/names.rs` `unescape_debug`, repeated) before hashing. One value then tokenizes
  the same in every spelling, and the encoding already covers control and bidi chars (below) and turns any reserved char
  inside the value into escaped text. Consequence: a literal `\u{fdd1}` (or any escaped-marker text, or a raw reserved
  char) inside a value always sits at least one depth deeper than its own span's closer, so depth matching makes it
  inert BY CONSTRUCTION. No separate "neutralize literal marker text" transform exists or is needed.
- **A span closes only on a closer at its opener's depth.** Why: a LITERAL `\u{fdd1}` at depth n spells exactly like a
  REAL closer at depth n+1 (verified on rustc 1.99.0: `format!("{:?}", "\\u{fdd1}")` prints `"\\u{fdd1}"`, the same
  bytes as a Debug-of-Debug marker, 2026-10-19). A closer at another depth inside an open span is value text; a span
  that reaches end of line without its own closer falls to rule 2.
- **Only a wrapper's own markers are legitimate.** Every producer (leaf wrappers, `LogDetail`, `LogError`, the FE
  bridge, `describeForLog`) replaces the reserved range (with `U+FFFD`) in every byte it writes outside its own spans,
  foreign text included (inside a span the content encoding already escapes it): an OS message or IPC error that echoes
  a hostile name carries a raw U+FDD1, and `LogDetail`'s Debug quoting would turn it into a depth-1 closer, the same
  depth as `LogDetail`'s real markers. Markers never cross IPC, a store, or an error `Display` (see "A marked string is
  log text, never data"), so a marked `String` only ever goes into a log macro, with ONE exception that must preserve
  them: `log_error!` formats its message into a `String` (`error_reporter/mod.rs` `report_error`), which re-logs it in
  the `Backtrace for [...]` DEBUG line (plain `{message}`, fine) and hands it to Flow B, whose auto note quotes it as
  `detail={:?}` via `LogDetail(&state.first_message)` (`auto_dispatcher.rs` ~354). Every FE error and every Rust
  `log_error!` reaches a report through that note, so if it replaced the reserved range like foreign text, every marked
  value in the note would fall back to the regex. It must Debug-quote the message WITH its markers (they shift one
  depth, which the parser reads natively; a raw stray marker from an unmigrated site becomes a stray at depth 1 and
  fails closed). Milestone 4 builds that quoting.
- **The FE mirrors Rust exactly.** `JSON.stringify` and `serde_json` don't escape noncharacters, and today neither does
  the bridge's `debugQuote` ("Quote like Rust's `{:?}`"). Make `debugQuote` escape the reserved range (and bidi-format
  chars) as `\u{…}` in lowercase hex without padding, the way Rust does (the parser and every sink match
  `u{fd[d-e][0-9a-f]}`), and give it a body-only sibling. Then a leaf placeholder writes `key="` + opener + body +
  closer + `"` (raw markers, depth 0, value escaped once, the same shape as a Rust `{:?}` of a leaf wrapper), and a
  `detail` placeholder `debugQuote`s the text `describeForLog` marked, so its spans arrive at depth 1 with their values
  escaped twice, exactly like `LogDetail` on the Rust side. Without this, an FE span would be depth 0 while its value
  sits inside one level of `debugQuote` escaping, and the same name would hash differently from FE and Rust. Parity
  means "unescapes to the same value", not byte equality: Rust's `<str as Debug>` also escapes grapheme-extend chars
  (every NFD accent) and its non-printable table, which `debugQuote` needn't copy; the safety-relevant set (`"`, `\`,
  control, bidi-format, the reserved range) must match exactly. A leaf's body is the RAW value body-escaped (its
  reserved chars escaped, never replaced), so a hostile name hashes the same from FE and Rust.
- **Control and bidi-format characters inside a wrapped value are escaped** the `{:?}` way (`\n` → `\n` text, U+202E →
  `\u{202e}`), so a name can't split a span across lines, forge a log line, or reorder the viewer's display.
  (`redact_text` splits on `\n` and redacts per line.) This falls out of the one content encoding above; it isn't a
  separate step.
- **Wrappers keep the site's format spec.** `{:?}` writes `"` + opener + body + closer + `"` (markers INSIDE the quotes,
  so the keyed grammar `key="…"` still bounds the value for a reader without the span pass, and the token is the same as
  the `{}` spelling); `{}` writes opener + body + closer. So a `path={:?}` site stays `path={:?}` and a prose site stays
  `{}`. Local text changes only by the invisible markers, plus `{:?}`-body escaping of a value in a `{}` site:
  `\`-escapes for a quote, backslash, or control char (rare), and `\u{…}` for every char Rust's `<str as Debug>` escapes
  beyond that, which includes combining marks. That last one is NOT rare: a name stored in NFD (common on macOS for
  names that came from HFS+ or some apps) reads `Cafe\u{301}` in the raw log file in a converted `{}` site. The local
  DISPLAY sinks (terminal, in-app viewer, MCP) already parse spans to strip the markers, so they also render the span
  body UNESCAPED by its depth: `Café` reads as `Café` there (lead's decision). Only a raw-file grep sees the escaped
  spelling; the logging docs' strip recipe covers it. No doc may claim "only invisible markers" for the raw file.

## Parser rules (the bundle span pass)

Runs first in `redact_with` (`redact/mod.rs`, the one loop both `redact_line`s share), on each line, before the regex
scan:

0. **Fast path.** A line with no `\xEF\xB7` byte pair (every reserved char's UTF-8 lead) and no `u{fd` substring skips
   the pass (`memchr::memmem`, already a desktop dep). That's every line today and every line without a user value after
   migration, so those pay two byte scans and nothing else. Sinks (terminal, viewer, console) use the same pre-scan
   before stripping, so the terminal's per-line cost stays flat.
1. A balanced span (opener … closer) is replaced by its token, hashed by kind through the existing typed internals
   (`fields.rs` `redact_typed_path`, `paths.rs` `redact_leaf`, `identity_token(.., TokenDomain::Volume, ..)`,
   `references.rs` `redact_host`; a user call is new and needs its own `TokenDomain`). Extensions and the allowlist
   behave exactly as for typed values today; the producer's kind beats the extension guess. `redact_typed_path` takes no
   kind today and decides the leaf inside each prefix branch (`redact_unix_home`, `redact_volumes`, …): thread an
   optional leaf-kind override through those branches rather than adding a parallel path redactor.
2. **An opener with no closer before the end of the line** (a cap, a `…`, a mid-line cut) redacts from the opener to the
   end of the line. Two refinements: when the opener sits inside a `key="…"` value that the field grammar delimits
   (escapes respected, the same `" (?: [^"\\\n] | \\ . )* "` the regex uses), the cut ends at that value's closing
   quote, so `detail="…"` stays a well-formed field and keeps its identity echo scrub and 200-char cap; and a trailing
   `\n` / `\r` stays (`redact_multiline` in `crash_reporter/mod.rs` feeds `split_inclusive` lines).
3. **A closer with no opener** redacts from the previous span boundary (or the start of the MESSAGE, after the
   `<ts> LEVEL target  ` header when it parses) to the closer, and on through the greedy hard-delimiter boundary from
   milestone 1 after it. Only hostile or broken text makes a stray closer, so the text glued after it
   (`evil\u{FDD1}rest/child.pdf`: `rest/child.pdf` has no `\B/` anchor and no path regex would claim it) is part of the
   damage.
4. **An opener inside an open span** extends the span to the next same-depth closer; the whole run is one value.
   Over-redaction, never a leak.
5. Span leaves feed the leaf echo scrub (`detail.rs::scrub_leaf_echoes`), so a bare repeat of a marked name on the same
   line goes too.
6. The regex fallback then runs over the WHOLE rewritten line (never per gap between spans), and **a path keeps its path
   context across a span**: the path branches accept a span token as a path chunk, so the text on either side of it
   still reads as one path. Why: a hostile name with a balanced pair inside a legacy `path.display()` line
   (`/Users/me/Photos/My \u{FDD0}a\u{FDD1} rest/child dir/x.pdf`) is indistinguishable from a real span, and today the
   path tail excludes `<` / `>`, so the match stops at the token and ` rest/child dir/x.pdf` ships whole (no `\B/`
   anchor). The same holds for a half-migrated site that glues a wrapped dir to an unwrapped name, and for every other
   fallback claim milestone 1 adds (the quoted Bonjour instance, bare `share=` / `*_volume=` labels, the greedy prose
   boundary): a span token is never a boundary, so a hostile balanced pair inside an `mdns_sd` WARN instance name
   (`"Alex\u{FDD5}s Mac\u{FDD1}Book Pro._companion-link._tcp.local."`) still goes whole. Tokens the span pass wrote stay
   verbatim (idempotent: bundles get re-redacted between preview and send); a segment that only LOOKS like a token plus
   extra text (`<file:abc>rest`) is a name and gets tokenized whole.
7. **The span pass and the regex run on the raw line AFTER the bundle's line filters** (time window, third-party level
   filter), which read the header from the raw line. Rule 3 may erase the message but never the header.

## Milestones

Ordering: 1 closes every leak in the bundles above (the `volume outage` lines via the transfer effort, smb2's line by
keeping its DEBUG and its `durable` records out until 8b) and touches nothing the transfer effort rewrites; 2 is the
smb2-side fix. 3–5 build the core (consumer first, then Rust and FE producers), 6 adds the ratchet. 7 migrates in
parallel by crate. 8 and 9 wait for the transfer effort to land. 10 locks it in. Each milestone lands green on its own.
Overall size: L.

### 1. Quick wins: close every known leak in the redactor (S)

**Scope**: `apps/desktop/src-tauri/src/redact/`, `error_reporter/bundle_builder.rs` + `tail_walker.rs`,
`logging/dispatch.rs` (file-chain provenance and the `mdns_sd` level), and three lines in
`crates/cmdr-index/src/indexing/` whose path is followed by a parenthesized stat: the two `scanned new dir` lines
(`watch/event_loop/verification.rs` ~119, `reconcile/verifier.rs` ~508) and
`MustScanSubDirs: reconcile complete for {} ({changes}…, {}ms{cpu})` (`reconcile/reconciler/rescan/mod.rs` ~675,
hundreds of lines per bundle; greedy would eat its `(+0 -0 ~0` counts up to the first `, `). ❌ Not
`transfer/volume_outage.rs`: it has FIVE `volume outage: {name} …` lines (~228, 245, 281, 335, 378), and the transfer
effort rewrites that file. The lead asks that effort to key all five as `volumeName={name:?}` while it's in there; this
milestone adds the redaction cell for the keyed shape.

- **Greedy fallback boundary.** When a prose path's end is uncertain, redact to the next hard delimiter: `, `, `; `,
  `"`, ` key=`, `->`, the `: ` seam, end of line. This replaces the sentence-end, lowercase-run, and closing-punctuation
  rules in `split_trailing_noise` for the decision of where the path ENDS. `(` and `)` are never delimiters.
- **`abs_path` accepts a word starting with `(`** (leak 2: `…/<dir> (Some Words) Name/2026-08-24/…`).
- **Third-party DEBUG/TRACE out of bundles, fail-closed** (lead decision): only records from our own crates keep
  DEBUG/TRACE in a bundle; INFO and above from anyone still ship, redacted. A bundle-time filter, so local files keep
  everything, except `mdns_sd`, which the file chain caps at Info too (~60% of a bundle's lines and disk; update the
  logging `CLAUDE.md` must-know that says the file chain is locked at Debug).
  - **Provenance is decided at WRITE time, from `record.module_path()`**, never from the target column. The file strips
    `cmdr_lib::` and our sites use short custom targets (`open_with`, `copy`), so a line's target can't tell ours from a
    third-party crate's; `module_path()` names the call site's crate whatever the `target:`. Ours: `cmdr_lib` and
    `cmdr_*` (FE lines are logged from `commands/logging.rs`, so `FE:*` counts as ours), plus `mtp_rs`; `smb2` joins
    only with the smb2 bump in milestone 8b (its DEBUG lines print paths positionally until then, leak 3). A missing
    `module_path()` (a `tracing` bridge, say) is third-party. The file line carries the verdict (encoding is the
    executor's choice); keep `parse_leading_iso8601`, the Flow B window parse, MCP `since`, and the coalescer key
    working. ❗ The encoding must fail closed for a line that carries NO verdict: the first bundles after the upgrade
    read rotated files written by 0.54 (full of `mdns_sd` DEBUG with instance names), and a downgraded build writes
    untagged lines. So an untagged DEBUG/TRACE line counts as third-party: tag OUR lines (or tag both and drop the
    untagged), never only third-party ones. Cost: our own pre-upgrade DEBUG drops from the first few bundles.
  - **smb2 INFO-and-above lines that print a relative path in prose** ship as INFO+ but no regex claims them: smb2
    0.32.0 (Cmdr's pin) logs `durable: {path} reclaimed on the new session…` at INFO on every resumed transfer (the
    ERR-VUFQZ scenario), plus `durable: {path} …` WARNs and an ERROR (`client/durable.rs` ~345, 353, 437, 449, 463).
    Until the milestone-8b bump, the bundle filter drops every `smb2::client::durable` record whatever its level
    (decided; lifted with the smb2 0.32.1 bump in 8b); milestone 2 quotes them. smb2's other INFO+ lines are covered by
    existing or new claims (`share=` below, the `dfs: namespace` UNC by `redact_remote_unc`; re-grep smb2 0.32.0 to
    confirm, read 2026-10-19).
  - **Continuation lines follow their header's verdict** (a multi-line third-party DEBUG record drops whole, the way
    `tail_walker` already drops an out-of-window record's continuations). Continuations before any parseable header, and
    a header that doesn't parse, drop (fail closed). Both bundle paths: `build_bundle_streaming` and
    `build_bundle_legacy_window`.
- **A Bonjour instance inside a quoted value is claimed whole**, from the opening quote to `._service._tcp.local.`
  (covers an `mdns_sd` WARN line, and any of ours).
- **Volume and share labels**: claim bare `share=`, `source_volume=`, `dest_volume=` values to the next hard delimiter
  (smb2 prints `share=Name` bare; `copy_between_volumes` prints both labels bare). The `volume outage` lines get keyed
  by the transfer effort (see Scope).
- **OS junk names ship verbatim**: `.DS_Store`, `Thumbs.db`, `desktop.ini`, `.localized`, `Icon\r`, `.Spotlight-V100`,
  `.Trashes`, `.fseventsd`. Not PII, useful for triage.
- **The two `scanned new dir` lines and the `reconcile complete for` line** log the path as a keyed field (`path={:?}`)
  so the greedy rule doesn't eat `(N entries, Nms)` (leak 7) or `(+N -N ~N, Nms, Nms CPU)`.

**Intentions**: close every shape in the bundles with no producer change outside the named lines. The greedy boundary
stays forever as the safety net for unmarked text.

**Landmines**:

- Greedy costs triage text on unconverted lines (the 98-report truncation in `redact/DETAILS.md` is that cost). Diff
  `fixtures/log-corpus.redacted.txt` before and after and read every changed line: a lost triage clue is acceptable, a
  token swallowing a following path's structure (`->` arrows) is not.
- Rule 4 of § "Finding the end of a path" (the `: ` seam skipping the trim) and the bare `path_field` rules
  (`end_of_bare_value`) already behave greedily; don't regress them.
- `$HOME`, `MB/s`, `and/or` must stay out of `abs_path` (the `\B/` anchor). A `(` led word must not let `(see /foo)` in
  prose start a path at the `(`.
- The junk-name list matches a whole LEAF exactly; `My.DS_Store.backup` is a name.

**Test plan** (red first; each cell a synthetic shape from the bundles):

- `path=$HOME/Pictures/Astro/2026/Summer (Lake House) 2026, entries=5`: nothing of `Lake House) 2026` ships.
- `copying $HOME/Pictures/A/B/Summer (Lake House) 2026 -> /Volumes/Backup/A/B/Summer (Lake House) 2026`: both sides
  tokenized, arrow kept.
- `Copy error: detail="Path: /Volumes/Backup/A/B/Summer (Lake House) Trip/2026-08-24/Sub/.DS_Store; Error type: …"`:
  every segment tokenized, `.DS_Store` verbatim.
- An `mdns_sd` DEBUG line is absent from the bundle; an `mdns_sd` WARN line with
  `name: "Alex's MacBook Pro (2)._companion-link._tcp.local."` ships `name: "<host:T>._companion-link._tcp.local."`.
- Provenance: a record with a custom short target from a `cmdr_*` module keeps DEBUG; a third-party DEBUG record whose
  target equals one of our short targets drops; a third-party DEBUG record with a multi-line message drops every line;
  an `smb2` DEBUG `tree: exclusive delete of Photos/2026/Summer (Lake House)/x.jpg refused` line drops; an `FE:*` debug
  line stays; an untagged DEBUG line (a pre-upgrade rotated file) drops, an untagged WARN line stays; an
  `smb2::client::durable` INFO `durable: Photos/Summer (Lake House)/x.jpg reclaimed on the new session…` drops.
- `source_volume=Macintosh HD, dest_volume=Family Photos, 2 sources`, `tree: connected share=Family Photos, tree_id=…`
  (smb2 logs it at INFO, so it ships; the same text from our own crate too), and
  `volume outage: volumeName="Family Photos" went away`: no label ships.
- `scanned new dir path="/x/y/z" (5 entries, 0ms)` keeps `(5 entries, 0ms)`;
  `reconcile complete for path="/x/y" (+0 -0 ~0, 6ms, 1ms CPU)` keeps its parenthesized stat.
- `com.example.app` folder and `.DS_Store` file: pinned at today's legacy guess for the former (documented), verbatim
  for the latter.
- The whole existing suite, `consistency_tests.rs`, and the corpus histogram stay green.

**DONE**: both bundles' leak shapes have red-then-green cells; corpus diff reviewed and committed; `redact/DETAILS.md` §
"Finding the end of a path" rewritten around the greedy boundary; leaks 1, 2, 3 (smb2's DEBUG and its `durable` records
stay out of bundles until 8b), 4, and 7 closed; leak 5 closed except the `volume outage` lines, which close when the
transfer effort keys them; 6 closes as sites migrate in milestones 7 and 8.

### 2. smb2 logs its paths and shares as quoted fields (XS, separate agent, smb2 repo)

**Scope**: `~/projects-git/vdavid/smb2`, by a SEPARATE agent working there and acting for the crate's interests, allowed
to release when solid. ❗ The transfer effort's release (smb2 0.32.0, typed loss and close-on-drop) is already out, so
this needs its own release; fold it into the next one (smb2#24's, if that comes first) and let Cmdr bump in milestone
8b.

- `path={}` → `path={:?}`, `share={}` → `share={:?}` across the crate (~34 sites in `crates/smb2/src/client/`: `tree.rs`
  20, `stream.rs` 5, `pipeline.rs` 5, `resolve.rs` 3, `times.rs` 1; re-grep).
- `tree: exclusive delete of {} refused …` (`client/tree.rs` ~1030) becomes
  `tree: exclusive delete refused path={:?} …`.
- Every inline or positional path, name, or share in prose, at EVERY level, not only `key={}` fields: the five
  `durable: {path} …` lines (`client/durable.rs`, INFO/WARN/ERROR) become `durable: … path={path:?}`. Re-grep for
  `{path}`, `{name}`, `{share}`, `{}` fed a path, and `dfs: namespace {requested}` (a UNC, which Cmdr's
  `redact_remote_unc` already claims; quote it anyway).

**Intentions**: smb2 can't depend on `cmdr-fs` and shouldn't know about Cmdr's markers. Debug-quoted fields are good
hygiene for any consumer (unambiguous boundaries, no log-line forgery from a hostile name), and Cmdr's redactor already
reads quoted keyed values exactly. The crate agent judges the wording.

**Landmines**: smb2's own log-format tests, if any. Cmdr's bump touches `crates/cmdr-smb/Cargo.toml` + `Cargo.lock`, the
transfer effort's territory until it lands, so the bump is milestone 8b's, never this milestone's.

**Test plan**: in smb2, whatever its conventions ask. In Cmdr (after the bump, in milestone 8b, which is also when
`smb2` joins the DEBUG provenance allowlist): a redaction cell for
`tree: exclusive delete refused path="Photos/2026/Summer (Lake House)/x.jpg.cmdr-tmp-<uuid>"` ships no segment.

**DONE**: smb2 released with quoted fields; the version noted in the lead's report for the bump.

### 3. The span pass and sink stripping (S–M, desktop plus the `cmdr-fs` marker table)

**Scope**: a new `redact/markers.rs` (parser), wired as the first step of the shared dispatch; `logging/dispatch.rs`
`write_terminal_line` strips the marker range; the debug log viewer (`open-debug-log.ts` → file viewer) strips it when
it shows the log; the marker constants defined for Rust in `cmdr-fs`'s new `log_private.rs` (milestone 4 builds the
producer API around them; define the constants here so the parser has them): ONE `LogKind` enum with its opener char,
the closer, the reserved range, and a `LogKind::from_opener(char)` lookup, so producer and parser share a single table.
The parser maps each `LogKind` to its typed redact call; nothing in `redact/` lists code points.

**Intentions**: the consumer lands before any producer, so the first marked line ever written is already redacted
exactly. Inert until milestone 4.

**Landmines**:

- Every `redact_line` caller gets the pass (MCP, crash hook, `os_crash_report.rs`, `tools/quiet.rs`,
  `redact_panic_message`). Check each still passes its own tests.
- `detail="…"` fields are unescaped and re-escaped by `detail.rs`; a span inside one arrives in escaped form, and its
  token must leave the quoting valid. The 200-char detail cap runs AFTER span hashing, never before.
- The file viewer is generic: strip only for the debug log, by an explicit option, not by sniffing content.
  `test/e2e-playwright/debug-log.spec.ts` reads the viewer; keep it green. The viewer is backend-driven (rows, search,
  `SearchMatch.column` in UTF-16 units, tail mode; `file_viewer/CLAUDE.md`), so strip where the backend decodes rows for
  a session opened with the option, never in the FE renderer: otherwise search columns, highlights, and row wrap are
  computed on text the user doesn't see. Byte offsets (`ChunkEnd`, `end_byte_offset`) stay raw-file offsets. Say what
  copy and "save range" do (raw or stripped) and test it.
- The `CoalescingWriter` dedup key and the RAM gauge are unaffected (markers are in the message); verify.
- **`cmdr://logs` filters BEFORE it redacts** (`tools/resources/logs.rs` `select_log_lines`: `line.contains(filter)` on
  the raw line), so a loopback caller can probe whether a path appears by watching the line count, which defeats the
  "redaction is mandatory" comment there. Pre-existing; fix it here (filter the redacted lines) since this milestone
  touches every `redact_line` caller anyway, with a red cell first. Two costs to handle: (1) the window is 5 MiB
  (`LOG_TAIL_WINDOW_BYTES`), so walk the lines newest-first, redact, test the filter, and stop at `limit` matches rather
  than redacting the whole window; (2) a filter on a raw path or name no longer matches anything. That's the point, but
  it changes how agents debug locally: say in `docs/tooling/logging.md` and the MCP docs that a path filter needs the
  log file itself (agents with filesystem access read it directly).
- **Local grep on the log file.** Markers sit between `key=` and the value, so `rg 'path="/Users/x'` stops matching (a
  substring inside the value still matches). `docs/tooling/logging.md` gains a one-line strip recipe for reading or
  grepping the raw file (for example `perl -CSD -pe 's/[\x{FDD0}-\x{FDEF}]|\\+u\{fd[de][0-9a-f]\}//g'`), and says `less`
  shows the markers as `<U+FDD2>`.

**Test plan** (red first), every cell through both `redact_line` and `RedactionContext::redact_line`:

- A balanced span of each kind → the typed call's token; `<file:T>.pdf` for a file path, `<dir:T>` for a dir path
  whatever its extension (`com.example.app`), a junk leaf verbatim.
- The same value at depth 0 (raw markers, body escaped once), depth 1, and depth 2 → the SAME token.
- Unbalanced opener (cut by `…`, by a cap, at end of line) → redacted to end of line (rule 2; a trailing `\n` stays).
  Stray closer → redacted back to the previous span boundary or the message start, header kept, and forward through the
  greedy boundary (rule 3). Nested same-depth opener → one span to the next same-depth closer (rule 4).
- Hostile values (the line as a wrapper WOULD write it after escaping, and raw as a legacy site writes it): a name
  containing each reserved char, literal `\u{fdd1}` text, `\\u{fdd1}`, `"`, `\n`, `: `, `, `, ` key=`, `)`, `->`,
  token-looking `<dir:abc>`. None shortens a span; none ships a byte of the name.
- Depth: a raw span holding literal `\u{fdd1}` text, and a depth-1 span holding `\\u{fdd1}` text (a literal after one
  Debug), each stay one span. A depth-1 and a depth-2 span whose value ENDS in `\` (run 5 and run 10 before the closer:
  the value's backslash body-escaped d+1 times, plus the closer's own 2^(d-1)), and a depth-1 opener right after foreign
  text ending in `\` (run 3), close and open where they should (trailing-zeros depth), and hash the same as the raw
  spelling.
- The FE spellings (a leaf `key="␟body␟"` at depth 0, a span inside a `debugQuote`d detail at depth 1, an identity-keyed
  JSON pair inside that detail) of a value with `"`, `\`, and an NFD accent in it hash the same as the Rust spellings.
  Build these lines by hand from § "The FE mirrors Rust exactly" (the bridge changes in milestone 5, which then feeds
  its real output to a Rust cell).
- Fast path: an unmarked line comes back `Cow::Borrowed`; a micro-bench (or a criterion cell if the crate has one) of a
  corpus line with and without markers shows the pass costs nothing measurable on unmarked lines.
- Legacy lines with a hostile balanced pair inside a path (rule 6):
  `/Users/me/Photos/My \u{FDD0}a\u{FDD1} rest/child dir/x.pdf` and
  `/Users/me/\u{FDD0}a\u{FDD1}Real Name/child/Secret.pdf` ship none of `rest`, `Real Name`, `child`, `Secret`. A stray
  closer glued to a tail (`/Users/me/Secret Dir/evil\u{FDD1}rest/child.pdf`) ships neither `rest` nor `child` (rule 3).
- An unbalanced opener inside `detail="…"` ends at the field's closing quote, and the field keeps its identity echo
  scrub (a bare `server` value earlier in the detail goes).
- A span leaf repeated bare elsewhere on the line is scrubbed.
- Idempotence: redacting the output again changes nothing.
- Terminal line and viewer output contain no reserved chars, raw or escaped; the file write path is unchanged.

**DONE**: `redact/CLAUDE.md` gains the span pass as its first must-know (DETAILS § "Marked spans" with the parser rules
and the escape-depth decision); `logging` docs name which sinks strip.

### 4. The producer API, `LogDetail` marking, and exemplar sites (S–M)

**Scope**: `cmdr_fs::log_private`: ONE wrapper type, `LogPrivate<'a> { kind: LogKind, value: &'a … }`, with a
constructor per kind (`LogPrivate::file_path(p)`, `::dir_path(p)`, `::path(p)`, `::file_name(n)`, `::dir_name(n)`,
`::name(n)`, `::volume(n)`, `::host(n)`, `::user(n)`; names are the executor's call). One type, not five: the kinds
differ only in their opener, the ratchet check and reviewers recognize one name, and `LogDetail::marking` takes the same
values with no conversion. `Display` writes opener + `{:?}` body + closer; `Debug` writes the same inside quotes (see §
"The marker format": wrappers keep the site's format spec). `LogDetail` (today a tuple struct with a pub field,
`LogDetail(&str)`, constructed as `LogDetail(text)` at every site, printed through `Debug`) learns
`.marking(&[LogPrivate])`, keeping the tuple constructor so unmarked sites don't churn: it first replaces the reserved
range in the foreign text, then wraps every occurrence of the values the caller handed the OS or library, then
Debug-quotes the whole (so its spans land at depth 1), and its 1 KiB cap never cuts inside a span (close it, or cut
before it). The Flow B auto note (`error_reporter/auto_dispatcher.rs` ~354) stops using plain `LogDetail` for
`first_message` and Debug-quotes it with its markers kept (a `LogDetail` sibling for our own already-formatted log text:
caps without cutting a span, replaces nothing; § "Only a wrapper's own markers are legitimate"). Convert the exemplar
sites: `cmdr-local` `list_directory_core`, `instance_lock`, `config`, `fda_probe`. (The `copy_volumes_with_progress`
line in `cmdr-operations/src/transfer/volume/copy.rs` is transfer territory: milestone 8a.)

**Intentions**: a log site states what each value IS; the bundle never has to guess. Wrappers borrow, allocate nothing
on the happy path where they can (write through the `Formatter`), and are useless outside formatting.

**Landmines**:

- A wrapper's output must never become a path: no `From<LogPrivate> for PathBuf`, no `to_string()` helper,
  `#[must_use]`. `ToString` comes free with `Display`, and `disallowed-methods` can't target a blanket impl for one
  type, nor would it reach the crates with their own `clippy.toml` (`cmdr-listing`, `cmdr-operations`,
  `cmdr-operation-log`) unless each copies it. So: the milestone-6 Go check also flags `.to_string()` / `format!` of a
  `LogPrivate` outside a log macro.
- `Path::display()` is lossy for non-UTF-8 names; the wrapper prints the same lossy form (no new information).
- `LogDetail::marking` with overlapping or nested values (a dir and a file inside it): mark the longest match first.
  Foreign text often spells the value differently (macOS hands back NFD; a shell quotes `'\''`); an unmatched occurrence
  falls to the detail field's existing redaction and echo scrub, which is fine. Since the foreign text's reserved chars
  are replaced FIRST, match against each value with its reserved chars replaced the same way, or a hostile name never
  matches its own echo. Plain `LogDetail` (no `.marking`) also replaces the reserved range from now on: an OS message
  echoing a hostile name would otherwise forge a depth-1 closer.
- A value marked inside `LogDetail` is body-escaped twice, so locally a quote or backslash in it reads `\\\"` / `\\\\`
  where it read `\"` / `\\` before. Rare in real names; accepted, but don't call it "only invisible markers".
- **A wrapper owns a WHOLE value.** Wrap the full path, never a suffix glued to unwrapped text (`"{}/{}"` with one side
  wrapped): build the joined path and wrap that. Keep the site's `key=` AND its format spec (`path={:?}` stays `{:?}`
  with the wrapper, which keeps the quotes), so the keyed-field grammar still bounds the value for any reader without
  the span pass (an older Cmdr reading newer rotated files after a downgrade).
- Performance: these sit on hot Debug paths (listing, indexing). No allocation: write the body through the `Formatter`
  (`str::escape_debug` differs from `<str as Debug>` on grapheme-extend chars, which NFD names are full of, so use a
  `fmt::Write` adapter around `<str as Debug>::fmt` that drops the outer quotes, and pin the equality in a test). A
  record below the active level never formats its args, so the wrappers cost nothing on filtered-out lines.

**Test plan** (red first): wrapper output for each kind, with hostile values (each reserved char, literal `\u{fdd1}`,
control chars) round-tripping through the milestone-3 parser to the right token; `LogDetail` cap at every byte offset
around a span never yields a span the parser leaks; exemplar sites produce lines whose bundle form matches a golden; the
Flow B auto note built from a marked `log_error!` message (a leaf span and a `LogDetail` span, plus a raw hostile U+FDD1
from an unmigrated `{}`) carries the spans one depth deeper, each hashing to the same token as on the log line, and the
hostile char fails closed.

**DONE**: `cmdr-fs` `CLAUDE.md` + `DETAILS.md` describe `log_private`; `redact/CLAUDE.md`'s "log a relative path as
`key={:?}`" must-know becomes "wrap it in `cmdr_fs::log_private`" (keyed quoting stays valid for crates that can't
depend on `cmdr-fs`); `docs/tooling/logging.md` § "Usage in Rust" shows the wrappers.

### 5. The FE bridge marks values, and transfer sites log the typed error (S)

**Scope**: `log-bridge.ts` `formatMessage` wraps every leaf `REDACTOR_KEY_BY_PLACEHOLDER` value in the marker of its
kind, as `key="` + opener + `debugQuote` body + closer + `"` (the shape of a Rust `{:?}` of `LogPrivate`), and
`debugQuote` learns to escape the reserved range and bidi-format chars the way Rust does (§ "The marker format": "The FE
mirrors Rust exactly"); `describe-for-log.ts` marks the values of identity-keyed JSON pairs (`"path"`, `"name"`,
`"server"`, …) the same way; the devtools console sink strips (raw and escaped spellings); a parity test pins the TS
marker constants to the Rust ones. The three `getTechnicalDetails(error)` log sites under `file-explorer/pane/`
(`dialog-state.svelte.ts` ~471 and ~914, `adopted-operation.svelte.ts` ~210) log the typed error itself (`{error}`), so
`describeForLog` renders the payload with its path marked. The fourth site, `transfer-progress-state.svelte.ts` ~424,
sits in the transfer effort's FE directory: it moves to milestone 8a. `getTechnicalDetails` stays UI-only.

The identity key set grows to cover what the typed errors actually carry. `WriteOperationError` alone has `path`,
`name`, and `volumeName`, but also `source`, `destination` (`destination_inside_source`), `first` / `second`
(`duplicate_source_names`), `entry` (an archive entry name, relative, so no regex catches it), and `deviceName`
(`read_only_device`, a phone's name). Today they reach the log as prose; logged as `{error}`, each is a bare JSON pair
that neither `describeForLog` nor Rust's `redact_json_identity_pairs` (`detail.rs`) keys on. Census the `TypedFailure` /
IPC error payload fields once, give each path or identity field a kind in `describeForLog`, and add the same keys to the
Rust regex as the fallback. Fields with no `LogKind` (`volumeId`, `serverId`, `deviceId`) stay on the Rust regex alone.

**Intentions**: FE values get the same exactness as Rust ones through the one seam #487 built.

**Landmines**:

- `describeForLog` caps at 1,000 UTF-16 units (`text.slice`): the cap must not cut inside a span (or must close it).
- **No FE input carries legitimate markers** (IPC payloads are UI data, so Rust never marks them). `describeForLog`
  replaces the reserved range in every string it renders OUTSIDE its own spans (error messages, unmarked JSON values,
  keys); any reserved char arriving over IPC is a hostile name. Inside a span (a marked JSON value, a leaf placeholder)
  the raw value is body-escaped instead, like Rust, so the bridge must body-escape a leaf's raw string, not
  `describeForLog`'s replaced rendering of it.
- **`detail`-mapped placeholders (`detail`, `error`, `err`, `result`) are containers, not leaves.** Wrapping one as a
  leaf would replace the markers `describeForLog` just wrote and hash the whole detail as one name: no leak, but the
  triage text is gone.
- The bridge's `(×N, deduplicated)` suffix and the throttle warning sit outside spans.
- **Mark JSON pairs AFTER `JSON.stringify`, never in its replacer.** A replacer's return value gets JSON-escaped, so the
  body would be escaped twice (JSON over `debugQuote`) at depth 0, and a value with `"` or `\` would hash differently
  from Rust. Rewrite the serialized pair instead: JSON-unescape the value, emit `"key":"` + opener + body + closer +
  `"`. The JSON stops being strictly valid for such a value; it's log text, and the Rust JSON-pair regex reads
  `(?:[^"\\]|\\.)*` either way.

**Test plan** (red first): `log-bridge.test.ts` cells per placeholder kind, hostile names (raw reserved chars, literal
`\u{fdd1}` text that `debugQuote` would double), a typed error whose IPC message carries a raw U+FDD1 (replaced, never a
closer), an `{error}` placeholder whose inner identity spans survive; `describe-for-log.test.ts` for JSON identity pairs
and the cap; a Rust redaction cell fed the exact line the bridge produces for a transfer `WriteOperationError` with
`Summer (Lake House) Trip/.DS_Store` ships no name, and one each for `duplicate_source_names`,
`archive_entry_name_refused`, and `read_only_device` ships none of `first`, `second`, `entry`, or `deviceName`.

**DONE**: `apps/desktop/src/lib/logging/CLAUDE.md` + `DETAILS.md` describe marking (the "name a placeholder from
`REDACTOR_KEY_BY_PLACEHOLDER`" rule stays, now because it picks the marker kind).

### 6. The ratchet: a check that flags unmarked values in log macros (S)

**Scope**: a Go scanner check, `desktop-rust-log-private-values` (or a better name), modeled on `lock-poison`: per-file
counts in `desktop-rust-log-private-values-allowlist.json`, seeded from the current state, shrink-wrapped by local runs,
`reason` mandatory. It flags, inside a log macro invocation (multi-line aware, `log::` and bare, `log_error!`):
`.display()`, `.to_string_lossy()`, and a positional or inline argument whose name reads as a path or identity (`path`,
`dir`, `file`, `name`, `folder`, `root`, `dest`, `source`, `share`, `volume`, `label`, `host`, `server`, `user`, with
prefixes and suffixes), unless wrapped in `LogPrivate` or `LogDetail` (and `LogError` once milestone 9 adds it). A
per-line opt-out with a reason covers a `name` that isn't the user's (a setting, a thread). It also flags a `LogPrivate`
turned into a `String` outside a log macro (`.to_string()`, `format!`), since a marked string must never become data
(milestone 4). It scans every Rust workspace crate (`apps/desktop/src-tauri/` and `crates/*`, the off-limits ones
included: seeding their allowlist entries edits only the JSON), so milestones 7 and 8 can drive it to empty; `vendor/`
is out, and `fsevent-stream` per milestone 7c. Wire into CI and `--fast`.

**Intentions**: every converted site stays exact, every new site starts exact, and the allowlist IS the migration's
progress bar. Errors built with `format!` are milestone 9's job, not this check's.

**Landmines**: `runnerDataInputs` for the JSON, `rustScanInputs` for the inputs (`scripts/check/checks/CLAUDE.md`); test
files out of scope; macro bodies with nested parens and string literals containing `(`. Leak 6 (the kind tag) closes as
sites migrate to `::file` / `::dir`.

**Test plan**: Go unit cells for each flagged shape, each non-flagged shape (wrapped, opted out, a test file), a
multi-line macro, a string literal with parens; staleness and shrink-wrap behavior.

**DONE**: the check runs in CI and `--fast`; `scripts/check/checks/DETAILS.md` lists it; the allowlist is seeded.

### 7. Migrate log sites, parallel by crate (three agents, M each)

Each batch drives its files' allowlist entries to zero: `LogPrivate` with the right kind wherever the producer knows it,
`LogDetail(text).marking(&[…])` for foreign text that echoes values the caller handed over. Cap concurrency at three.

**Parallel mechanics**: the batches touch disjoint crates, but all three edit ONE allowlist JSON, and the check's
shrink-wrap rewrites the whole file. Run each batch in its own worktree (`new-worktree.sh`), have each commit only its
own entries' removal, and let the lead land them one after another (a later batch rebases and re-runs the check to
re-shrink). Three agents running `pnpm check` in one worktree also race on the JSON and on `target/`.

- **7a. `cmdr-index`** (~315 sites). Split by subdirectory if one agent can't finish it inside an M budget.
- **7b. The desktop crate** (~200): `network`, `file_system`, `volumes`, `volumes_linux`, `commands`, `search`, and the
  long tail. Our own mDNS lines mark instance names as `host`.
- **7c. Small crates**: `cmdr-mtp`, `cmdr-local`, `cmdr-fs` (except `src/volume/`, off-limits until the transfer effort
  lands: its `host/runtime.rs` and `liveness.rs` sites move to 8b), `cmdr-listing`, `cmdr-archive`, `cmdr-git`,
  `cmdr-operation-log`, `cmdr-http`, `cmdr-update-trust`, `cmdr-collation`, and the two dev CLIs (`index-query`,
  `operation-log-dump`) if the check flags anything there. The vendored `fsevent-stream` fork either converts or leaves
  the check's scope by an explicit exclusion; say which. (mtp-rs 0.33.0, Cmdr's pin, logs no path or name arguments,
  read 2026-10-19; it gets the milestone-2 treatment by its own agent if that changes.)

**Intentions**: mechanical, but a judgment per site on kind. When a value's kind is unknown, use the neutral form; never
guess.

**Landmines**: other efforts land in these crates meanwhile (`cmdr-git`, `cmdr-fs` volume errors): branch from current
`main`, rebase before handing back, don't stomp. A converted line's LOCAL text changes only by invisible markers (plus
`{:?}`-body escaping in a `{}` site, which includes `\u{…}` for NFD combining marks, and one more escape layer for a
value marked inside `LogDetail`; § "The marker format"); anything else that changes is a bug. Lines that `LogRollup`
keys or the coalescer dedups must keep their key stable.

**Test plan**: per batch, two or three representative sites get a golden cell (raw line → bundle line). The check is the
coverage proof.

**DONE**: the batch's allowlist entries are gone; the touched crates' docs mention the convention where they had a
logging must-know.

### 8. Late batches, after the transfer effort lands (two agents, M each)

**Scope**: **8a** `cmdr-operations` (all of it, transfer included, ~200 sites), plus the FE
`transfer-progress-state.svelte.ts` ~424 site from milestone 5, and the `copy_volumes_with_progress` exemplar from
milestone 4 (log the typed error, with the same Rust redaction cell for a `WriteOperationError`, including the transfer
effort's new `ServerConnectionLost { path, volume_name, … }`). **8b** `cmdr-smb`, `cmdr-sftp`, `cmdr-webdav`, `cmdr-s3`,
`cmdr-adb`, `cmdr-fs/src/volume/` (~100), plus the bump to milestone 2's smb2 release, and `smb2` joins the DEBUG
provenance allowlist from milestone 1 and loses its `smb2::client::durable` drop in the same commit (not before: those
lines print paths positionally until that release).

**Intentions**: same as 7. These areas move under #489/#490 now; migrating after avoids rebasing 300 log edits across a
rewrite.

**Landmines**: the transfer effort may have added lines; the check catches them. `source_volume=` / `dest_volume=`
become `LogPrivate::volume`, and milestone 1's bare-label claims stay as fallback. If milestone 2's smb2 release isn't
out yet, 8b migrates its crates and leaves the bump, the provenance change, and the `durable` drop for a follow-up
commit; never flip provenance without the bump.

**Test plan**: as milestone 7, plus in 8b milestone 2's Cmdr cell (`tree: exclusive delete refused path="…"` ships no
segment), an `smb2` DEBUG line with a quoted `path=` now ships redacted, and an `smb2::client::durable` INFO line in the
new quoted shape ships with its path tokenized.

**DONE**: the allowlist is empty; `smb2` is in the provenance allowlist and its `durable` drop is gone.

### 9. Path-bearing errors log through `LogError` (M–L, after the transfer effort)

**Scope** (lead decision: no marking `Display`): census first: every error type whose fields hold a path or name
(`VolumeError`'s path strings, `PathedVolumeError`, the transfer `WriteOperationError`, listing and git errors, the 65
`format!(… .display())` error strings). Each converted type writes its message ONCE, against a path style: `Display`
renders plain, and `cmdr_fs::log_private::LogError(&e)` renders the same text with its typed path and name fields
wrapped (for example one `fn fmt_with(&self, f, style)` both call, so the two texts can't drift). An optional
`cmdr_fs::UserPath` newtype (`#[serde(transparent)]`, plain `Display` AND plain `Debug`) types which fields are paths.
Log sites switch `{e}` to `{}` with `LogError(&e)` for converted types. Convert by type, smallest blast radius first;
`VolumeError` last (~380 construction sites).

**Intentions**: a converted error logged through `LogError` marks its own paths, and nothing a user sees, stores, or
receives over IPC ever carries a marker, by construction rather than by an accessor audit.

**Landmines**:

- **The known gap**: an error formatted deep in a chain (`anyhow` context, a `source()` that isn't a converted type, a
  `format!` into a `String` before it reaches a log macro) renders plain and falls to the greedy fallback. `LogError`
  walks `source()` for converted types it can recognize; the rest is documented, not hidden.
- **`Debug` stays plain too.** `{e:?}` sometimes reaches IPC messages (`format!("{e:?}")`); a marking `Debug` would leak
  markers into UI the same way a marking `Display` would.
- `LogError`'s non-span text (the error's own words, and any foreign `source()` text) gets the reserved range replaced,
  like every producer.
- The transfer effort adds `VolumeError::transport_fault`; land after it.
- `specta` bindings stay byte-identical (`desktop-bindings-fresh`).

**Test plan**: per converted type, `Display`, `Debug`, and `serde_json` carry no reserved char, `LogError` marks every
path field and round-trips through the span pass to the right kind; `LogError` and `Display` agree once markers are
stripped; a hostile path in a field can't close the span early.

**DONE**: census types converted or explicitly deferred to an issue with reasons; docs in `cmdr-fs` and the error
modules.

**Done note**: `cmdr_fs::log_error` (`LogError`, `ErrorLogText`, `ErrorText`, `write_plain`; no `UserPath` newtype was
needed). Converted: `VolumeError`, `FriendlyGitError`, both `VolumeScanError`s, `MutationError` (operations and
archive), `ReconnectError`, `MtpConnectionError`, `ArchiveError`, `ViewerError`, `FlushFailure`, `NotWalkable`,
`SizeError`. `WriteOperationError` logs as `log_detail()` (the frontend's JSON in `detail=`) rather than through
`LogError`. Sites found by compiling with each type's `Display` removed, then `{:?}` sites by a scan of files using
them. Deferred with reasons: #568 (error text flattened into strings before a log macro, ~72 `format!` error strings,
`io::Error::other(e.to_string())`, third-party errors). Cells: `redact/error_producer_tests.rs`,
`cmdr-fs/src/log_error_test.rs`.

### 10. Lock-in (S)

**Scope**: the check runs with no allowlist (strict) (no clippy `Path::display` ban: lead decision, the ratchet covers
log macros); rewrite `redact/CLAUDE.md` so the span pass leads and the regex reads as the fallback for foreign and
third-party text; update `docs/tooling/logging.md`; file what's left as issues.

**DONE**: an end-of-effort conformance review against the invariants below, plus a docs audit of every touched
`CLAUDE.md`.

**Done note**: the check is strict (allowlist and shrink-wrap code removed). Its name classifier now also reads `key`,
`remote`, `target`, `url`, `addr`, `device`, `query`, and kin as user values (`from` / `to` only as a whole name), with
`_id` / `_kind` / size-like fields as scalars; the leaks it surfaced (keychain keys, Quick Look's path, SMB `addr=`,
WebDAV's root URL, an S3 destination key, archive sidecar names, adb stderr, a NetFS mount URL) are marked. `cmdr-http`
can't depend on `cmdr-fs`, so its PAC line logs a keyed `host={:?}` and opts out. `redact/CLAUDE.md` leads with marking;
the regex reads as the fallback. Conformance: every invariant below has cells (`redact/markers_tests.rs`,
`bundle_leak_tests.rs`, `fe_bridge_tests.rs`, `error_producer_tests.rs`, `logging/dispatch_tests.rs`,
`tools/tests/resource_log_tests.rs`, `logging/provenance.rs`); "the ratchet only shrinks" became "the check is strict".

## Invariants (the conformance review checks these)

- **No name ships.** Every leak shape from ERR-VEZUB / ERR-VUFQZ has a cell; every hostile-marker case has a cell.
- **Fail closed in both directions**: an unbalanced opener redacts to end of line (or to its `key="…"` field's closing
  quote), a stray closer redacts back to the last span boundary or the message start (never the header) and forward
  through the greedy boundary, a nested same-depth opener widens the span.
- **Escaping**: every producer (leaf wrappers, `LogDetail`, `LogError`, the FE bridge, `describeForLog`) replaces the
  reserved range in everything it writes outside its own spans, and writes every span value as the `{:?}` body (which
  escapes control, bidi-format, and reserved chars). No producer ever passes through markers it didn't make, except the
  Flow B auto note (below), which quotes our own already-marked log message.
- **Depth-matched spans**: a span closes only on a closer at its opener's escape depth, read as
  `1 + trailing_zeros(run)` of the maximal backslash run; a depth-d span's value is unescaped d+1 times.
- **Path context crosses spans**: text on either side of a span inside a path still faces the path branches as one path;
  a stray closer's glued tail is redacted.
- **One spelling, one token**: raw and any depth of `{:?}` escaping hash to the same token.
- **One policy**: unsalted and salted `redact_line` run the same span pass.
- **Local logs keep raw values**; terminal and debug log viewer strip markers; the file keeps them.
- **A marked string is never a path, a persisted value, IPC payload, or UI copy**; error `Display` and `Debug` stay
  plain.
- **Only our own crates' DEBUG/TRACE reaches a bundle** (provenance from `module_path()` at write time; a line with no
  verdict counts as third-party; `smb2` only after its quoted-fields bump, and its `durable` records at no level until
  then); continuation lines follow their header; INFO+ from anyone does, redacted.
- **The Flow B auto note keeps `log_error!`'s markers** (one depth deeper); it's the one producer that quotes marked
  text instead of replacing reserved chars.
- **`cmdr://logs` filters on redacted text**, never raw.
- **The check is strict**: no allowlist, any unmarked value in a log macro fails.

Decisions waiting on David: § "Status and decisions for David" at the top.
