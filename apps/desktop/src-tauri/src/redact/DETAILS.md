# Redact: details

Depth and rationale. `CLAUDE.md` holds the must-knows and the pattern table.

## Pattern table

| Group | Matches | Rewrites to |
| --- | --- | --- |
| `unix_home` | `/Users/<user>/...`, `/home/<user>/...` | `$HOME/<allowlisted-parent-or-dir>/<file>.<ext>` |
| `win_home` | `C:\Users\<user>\...` | `$HOME\<allowlisted-parent-or-dir>\<file>.<ext>` |
| `unix_system` | `/tmp/`, `/var/`, `/private/`, `/opt/` | prefix kept; tail walked with same shape rules |
| `volumes` | `/Volumes/<label>/...` (spaces allowed) | `/Volumes/<volume>/<allowlisted-or-dir>/<file>.<ext>` |
| `media` | `/media/<label>/...` (spaces allowed) | `/media/<volume>/<allowlisted-or-dir>/<file>.<ext>` |
| `abs_path` | any other absolute path, two segments or more, not after a word char (`/srv/a`, `/DCIM/x.jpg`) | system root kept (`/Applications`), every other segment tokenized |
| `remote_url` | SFTP/SSH/WebDAV/S3/HTTP(S)/SMB URL, with or without userinfo | scheme + hierarchy + port + address class + conservative extension; identities tokenized |
| `unc` | `\\host\share\...` | `\\<host>\<share>\<redacted tail>` |
| `url_userinfo` | another scheme's `scheme://user[:pass]@host/...` | same complete component redaction as recognized URLs |
| `bare_userinfo` | `//user[:pass]@host/...` (no scheme) | SMB-shaped component redaction without inventing a scheme |
| `detail_field` | external text: `detail="…"`, `stderr="…"`, `stdout="…"` (Debug-quoted) | redacted, keyed echoes scrubbed, capped (200 chars) |
| `path_field` | a keyed field: `path=`, `smb_path=`, `from=`, `to=`, `file=`, … (see the regex) | relative value walked in place; absolute value handed to the branches above |
| `derived_id` | current `smb`/`sftp`/`webdav`/`s3`/`adb`/`mtp`/`vol`/`path` ID with 16-hex digest | scheme kept, opaque ID tokenized; MTP storage number kept |
| `manual_server_id` | `manual-<address-derived name>-<port>` | `manual-<server-id>-<port>` |
| `email` | `local@domain.tld` | `<email>` |
| `account` | `user=`/`username:` fields | `user=<user>`, `None` untouched |
| `identity_field` | quoted `host=`, `server=`, `share=`, `volumeName=`, `source_volume=`, `dest_volume=`, the ID keys | the value tokenized in its identity's domain |
| `label_field` | bare `share=`, `source_volume=`, `dest_volume=` (`share=Family Photos, …`) | `share=<share:T>`, value ends like any bare field value |
| `quoted_bonjour` | a quoted instance, any name (`"Alex's MacBook Pro (2)._companion-link._tcp.local."`) | `"<host:T>._companion-link._tcp.local."` |
| `bonjour_instance` | `Name._smb._tcp.local.` (DNS-SD instance) | `<host:T>._smb._tcp.local.`, T shared with `server="name"` |
| `bonjour_service` | `_smb._tcp.local.` (a public service type) | kept verbatim |
| `mdns` | `<label>.local`, every label (`nas.home.local`) | `<host>.local` |
| `ipv4` | dotted-quad, valid octet ranges | `<ipv4-private:T>` (class label, as keyed `host=`) |
| `ipv6` | full + compact forms (`::1`, `fe80::1`) | `<ipv6-link-local:T>` etc. |

`T` is the report token; unsalted `redact_line` writes the same token without it (`<host>`, `<ipv4-private>`).
| `mtp_owner` | `<Owner>'s <Model>` device names | `<mtp-owner>'s <Model>` (model kept) |

`mtp_owner` needs a known model word (`iPhone | iPad | Pixel | Galaxy | OnePlus | …`) right after the `'s `, which is
what keeps contractions and module paths out of it. `SAFE_PARENT_DIR_NAMES` is the parent-dir allowlist: `Documents`,
`Downloads`, `Desktop`, `Library`, `src`, `Pictures`, `Movies`, `Music`, `Public`, `AppData`, `Application Support`.

`OS_JUNK_NAMES` (`paths.rs`) is a second allowlist, for whole segments at any depth: `.DS_Store`, `Thumbs.db`,
`desktop.ini`, `.localized`, `Icon\r`, `.Spotlight-V100`, `.Trashes`, `.fseventsd`. The OS or a sync tool writes them,
they carry no one's data, and a walk that tripped on one is a triage clue. Exact match only (raw or `{:?}`-escaped):
`My.DS_Store.bak` is a name.

## Pattern overlaps

- **Dispatch order mirrors the regex alternation order.** `remote_url` must claim recognized schemes before the generic
  userinfo branch, and IDs must be claimed before embedded address patterns can split them. Don't reorder without
  re-checking these overlaps.
- **`bare_userinfo` captures a leading delimiter (`^` or one whitespace) into `bare_lead` and re-emits it.** The regex
  crate has no lookbehind, so this anchoring is how the scheme-less `//user:pass@host` shape (built by the macOS
  `smbutil` / Linux `smbclient` fallbacks) avoids grabbing the `//user@host` tail inside a scheme'd `http://user@host`
  (handled by the earlier URL branches). Don't drop the lead capture.

## Marked spans

A producer that knows what a value is wraps it between its kind's opener and the closer, Unicode noncharacters from
U+FDD0–U+FDEF defined once in `crates/cmdr-fs/src/log_private.rs` (`LogKind`, `CLOSER`, `RESERVED`; nothing here lists
a code point). `markers.rs` reads them first, on every line, in the shared `redact_with`, so both `redact_line`s run it.

**The content encoding.** A span's value is always written as the BODY of `{:?}` (what `<str as Debug>` writes between
its quotes), raw markers around it, in a `{}` site and a `{:?}` site alike (the latter inside quotes). Every `{:?}` the
marked text goes through afterwards (a keyed field, `LogDetail`, a derived `Debug`) escapes the markers too: a
noncharacter prints as `\u{fdd1}` (verified on rustc 1.99.0, `debug_formatting_escapes_every_reserved_char_in_lowercase_hex`,
2026-10-19). So a span at depth d holds its value escaped d+1 times, and the pass unescapes it d+1 times before hashing.
One value tokenizes the same in every spelling, and control, bidi, and reserved chars inside a value are always escaped
text, one depth deeper than the span's own closer.

**Decision: depth comes from the backslash run's trailing zeros, not its length.** A raw marker is depth 0. An escaped
one behind a MAXIMAL run of r backslashes is depth `1 + r.trailing_zeros()`: at depth d the marker's own escape is
2^(d-1) backslashes, and every literal backslash in front of it arrives as a multiple of 2^d, so r = 2^(d-1) × odd.
Counting the length instead reads a value ending in `\` (run 5 at depth 1, 10 at depth 2) or foreign text ending in `\`
before an opener (run 3) as another depth, and the span never closes. The lexer (`cmdr_fs::log_private::markers`,
which the producers' span-aware caps read too) accepts any escape `unescape_debug` would decode to a reserved char
(any case, 1–6 hex digits), and the fast path skips a line with neither the reserved chars' UTF-8 lead bytes (`EF B7`)
nor a `u{`.

**Why a span closes only at its own depth:** a literal `\u{fdd1}` at depth n spells exactly like a real closer at
depth n+1 (`format!("{:?}", "\\u{fdd1}")` prints the same bytes as a Debug of a Debug of the marker). Matching depth
makes every literal inert by construction.

The parser rules, each pinned in `markers_tests.rs`:

1. A balanced span (an opener, then the next closer at its depth) is hashed by kind through the same typed calls typed
   report values use: `redact_typed_path` with the leaf kind threaded through every path branch (`paths::Leaf`), so a
   file path keeps its extension and a dir path never gets one (`com.example.app` reads `<dir>`); `redact_leaf` for
   names; the volume domain for volumes (shared with `share=`); `redact_host`; the userinfo domain for users, so a user
   span correlates with a `user=` field. Junk names and the allowlists behave as for any typed value.
2. **An opener with no closer** redacts to the end of the line, a trailing `\n` / `\r` kept, or to the closing quote of
   the `key="…"` field it sits in, so a cut `detail="…"` stays one field: still capped, still echo-scrubbed. The closer
   search is bounded the same way: a legit span's body is escaped, so it never holds the field's closing quote.
3. **A closer with no opener** redacts back to the previous span's end, or to the message start (after `<ts> LEVEL
   target  `; the header the bundle filters read never goes), and on through the name glued after it, to where the
   greedy prose boundary ends a path (`evil<closer>rest/child.pdf`: no regex would claim `rest/child.pdf`).
4. **An opener inside an open span** widens it to the next same-depth closer: one value. A closer left over is stray.
5. Span leaves (and volume, host, and user values) feed the echo scrub like a path leaf or a keyed identity.
6. **A path keeps its path context across a span.** The regex reads a placeholder per span (`READER_SCRATCH` chars,
   which the path tails take as ordinary path chars), and `Spans::fill` puts the tokens in afterwards. So a hostile
   balanced pair inside an unmarked path (`/Users/me/Photos/My <pair> rest/child dir/x.pdf`), a bare label, or a quoted
   Bonjour instance still goes whole, and `abs_path` may start at a placeholder, for a wrapped dir glued to an unwrapped
   name. A segment, field value, or identity that IS one placeholder stays for `fill` (`token_for`, `identity_token`);
   one holding more text is a name and is hashed whole.

**Decision: placeholders, never tokens, during the regex.** Writing the tokens into the line first let the fallback
re-read the span's own output: `listed /Volumes/<volume:…>/<dir:…> in 3ms` matched `volumes` again once tokens were
path chunks, and the greedy boundary ate ` in 3ms` and rehashed the token. A placeholder can't be re-read, and only the
span pass writes reader-scratch chars: it consumes every reserved char in the line (each one is a marker), and typed
values and span values reach the walkers with reserved chars replaced by U+FFFD (`without_reserved`), so nothing else
can spell one. The output carries no reserved char, so a second pass is the plain regex over tokens (idempotent, except
for a path span whose every segment survives, like `/Library/Application Support`, which a second pass's greedy
boundary may extend into the prose after it: no leak).

Broken spans become `<redacted:T>` (bare `<redacted>`). Inside a detail field, `fill` runs before the 200-char cap.

**The display sinks** (the terminal, Help > View debug log) read the same markers through `display_line`: markers gone,
raw or escaped, and each balanced span's value decoded to itself (`Café`, not `Cafe\u{301}`), with only control and
bidi-format chars escaped so a name can't split or reorder the line. The log file keeps everything.

## Decision: remote structure survives, remote identities do not

Report delivery treats the complete URL/UNC reference as one privacy unit. The rewriter keeps the protocol, explicit port, separators,
segment count, conservative final extension, and an IP's broad class (loopback, private, link-local, unspecified, or
public). It tokenizes username, password, hostname/address, SMB share, every path segment, query keys and values, and
fragment. A `.local` suffix survives because it describes discovery scope, not the host label. Remote `Downloads` and
other local-folder allowlist words do not survive.

The one URL that survives whole is a production Svelte error, `https://svelte.dev/e/<snake_case_code>`
(`is_svelte_error_url`). It's the entire message of an uncaught frontend error and names a public error code, so
tokenizing it left reports saying only "some Svelte error" (ERR-DAN3Q needed a rebuild and a stack-offset lookup to
read `each_key_duplicate`). The match is exact: any userinfo, port, query, fragment, uppercase, or extra segment fails
it and the URL gets the normal treatment. Pinned by `svelte_error_code_urls_survive_but_only_in_their_exact_shape`.

`url::Url` handles valid authorities. Logs also contain malformed-but-recognizable values, so a lexical splitter covers
the same bounded `scheme://authority/path?query#fragment` shape when standards parsing rejects it. Percent-decoding is
for token identity and extension recognition only; decoded source text is never emitted. This keeps NFC/NFD and encoded
spellings correlated without turning malformed input into a raw-data escape hatch.

Derived IDs are redacted only here, at the diagnostic boundary. Current funnel IDs require a known scheme and their
exact lowercase 16-hex digest; MTP's numeric storage suffix and a manual server's port remain useful. The slug and digest
become one opaque report token because the slug is deliberately lossy and cannot be safely reverse-parsed into host,
account, and share. Functional ID generation, and the ID fields MCP resources print without redaction, remain
unchanged.

## Decision: one policy; the context only salts tokens

Unsalted `redact_line` (ordinary MCP logs, listing failures, the crash hook) and
`RedactionContext::redact_line` (crash and error reports) run the same dispatch: complete remote references,
structured identities, derived IDs, home roles, external-text fields and their cap. The only difference is the token:
`<dir>` against `<dir:T>`. `unsalted_api_runs_the_report_policy_with_bare_tokens` asserts the two agree, modulo hashes.

Why not keep MCP byte-compatible with the pre-report redactor: that took about eight `context.is_none()` branches
emulating the old scanner (rescanning inside new outer matches one character at a time, a URL span cut at the first
space), and the emulation itself caused a bypass. A space-containing URL swallowed the path after it, so a home folder
name shipped raw (cmdr-reports#30, pinned by `a_space_containing_url_never_hides_the_path_after_it`). The cost of one
policy is a stricter MCP output: URL paths, queries, and fragments are tokenized, addresses carry their class
(`<ipv4-private>`), and a `Documents` folder off `$HOME` reads `<dir>`.

Typed report structures call `RedactionContext::redact_path`, `redact_name`, `redact_volume_name`, and
`redact_volume_id` instead of converting themselves to prose. They preserve report-scoped token domains, and an
unrecognized or relative path fails closed by tokenizing every non-structural segment. A typed path owns its complete
value boundary: it dispatches directly to the home, mount, remote-reference, UNC, or relative rewriter and never calls
the prose scanner or `split_trailing_noise`. Raw typed values treat every token-looking segment as untrusted input and
tokenize it again. Only a path the line scanner reached (prose, or a quoted field) preserves existing generated tokens
(`preserve_redacted_segments`), because that scanner can be reprocessing its own output and must remain idempotent. These methods are lexical
only: bundle assembly performs no filesystem or network lookup.

## Decision: path-shape preservation + allowlist

The tradeoff is debuggability ("I can see this is a Documents path") against PII safety ("but I don't want to leak
project codenames"). The allowlist captures the dirs that are near-universal across users; anything custom collapses.
Net result: triagers can usually guess the failure context without seeing the user's secrets.

The well-known home folders (`HOME_ROLE_DIRS` in `paths.rs`: Downloads, Desktop, Documents, Pictures, Movies, Music,
Library, and `Library/Mobile Documents`, `Library/CloudStorage`, `Library/Application Support`) have a stricter rule:
they're fixed macOS names whose role decides TCC protection, and Cmdr gives Downloads special behavior. Redaction keeps
one only when the path branch proved `/Users/<account>/<role>`, `/home/<account>/<role>`, or the Windows equivalent, and
keeps it through deeper descendants (the longest role wins, so `$HOME/Library/CloudStorage/<dir:…>`). A custom local or
remote path that merely contains such a segment gets a token. This is lexical and non-blocking: the hot path never
resolves symlinks or touches a filesystem.

## Decision: an extensionless leaf reads as `<dir>`

`has_extension_like_suffix` decides whether a path's last segment becomes `<file>` or `<dir>`, so an extensionless file
(`id_rsa`, `README`, `Makefile`) is mislabeled `<dir>`. Accepted: Cmdr's logs are dominated by directory listings, so
guessing `<dir>` is right far more often on real triage data than the reverse.

What counts as an extension is deliberately narrow (`paths.rs::conservative_extension`): at least one letter, and
lowercase alnum up to five chars, uppercase up to four (camera-style `JPG`, `HEIC`), or a short list of known long ones
(`sqlite3`, `numbers`, …). A dot inside a name (`Anna.Kovacs`, `minutes.2026`) keeps nothing, so the tail of a name
can't ship as its "extension".

## Decision: MTP owner names redacted, model names kept

`mtp_owner` catches the common `<Owner>'s <Model>` shape (`John's Pixel 8 Pro`). The owner becomes `<mtp-owner>`; the
model phrase (`Pixel 8 Pro`, `iPhone 15 Pro`) is kept because model strings alone aren't identifying and are useful
diagnostic context. The pattern requires both a capitalized possessive AND a model word from a known set
(`iPhone | iPad | Pixel | Galaxy | OnePlus | Note | Tablet | Phone | Camera | ...`) right after the `'s `, which keeps
English contractions (`it's a Pixel`) and module paths (`cmdr_lib::mtp::device`) untouched. `That's Pixel 8 Pro` does
match, accepted as an over-redaction (rare phrasing without an article between `'s` and the model word).

## Decision: account names redacted, structured remote shares redacted

An SMB login has three parts in our logs, and they don't carry the same weight. The **account name** is a real
identifier, as personal as the email pattern, and it appeared verbatim in three places (`commands/network.rs` twice,
`crates/cmdr-smb/src/connection.rs` once), so `account` collapses it to `<user>`. A share inside an SMB URL or UNC path
is a recognized identity and gets a report-local `<share>` token, and so does a `share=` field, quoted or bare (`smb2`
prints `tree: connected share=Family Photos, …`). The bare volume labels `copy_between_volumes` prints
(`source_volume=`, `dest_volume=`) get the same treatment in the volume domain, so a share and the volume it mounts as
correlate. A bare NetBIOS name (`server=NASPOLYA`) is a known remaining gap.

`None` passes through because it isn't a name, and the difference between "no username in the mount info" and "a
username we then looked up in the Keychain" is exactly what a triager reads these lines for. That's why this can't be a
blanket `user=\S+` rewrite.

## Finding the end of a path

A space can be inside a path (`/Volumes/My Backup Drive`), inside a filename (`Invoice for Acme Corp.pdf`), or the gap
between a path and the prose after it. Nothing local to the character tells the three apart, so the regex over-matches
and `split_trailing_noise` decides where the path ends.

**Decision: greedy, to the next hard delimiter.** Every rule that tried to read the end off a word's shape shipped part
of a name, so the boundary no longer guesses. The history is the reason, so it stays here:

- Anchoring continuation words to `[A-Z0-9]` stopped `Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg` at ` at`, so every
  multi-word filename leaked its tail.
- A dotted word ended a sentence: `12. Summer trip/beach.jpg` shipped ` Summer trip/beach.jpg` (ERR-UK5NH).
- A `)` ended a sentence: `Summer (Lake House) 2026, entries=5` shipped ` 2026` (#491).
- A letter-led extension ended a filename, so `Summer.Trip 2026 photos` would ship ` 2026 photos`, and a trailing
  lowercase run was prose, so `/Volumes/x/summer trip: failed` once shipped `trip`.

What ends a prose path now, and nothing else:

1. **The `": "` seam, anywhere.** Nearly every caller formats `{path}: {message}`. macOS forbids `:` in a filename, so
   cutting there can't shorten a real name; on Linux a `: ` inside one is vanishingly rare.
2. **A word that starts a new reference**: one containing `://` (`https://u:p@a.com and https://x:y@b.com`), or a
   trailing `scheme:` the match stopped at before `//` (`/Volumes/d/f.txt and smb://host/…`). Handed back, the scanner
   claims the reference (§ "Why the scan resumes mid-match").
3. **`, `, `; `, or ` key=` after the last separator.** Before it, a deeper segment proves the path goes on
   (`Smith, John/x.pdf`, `Dr. Dre, Snoop (live)/track.mp3`), and cutting there would ship the rest: nothing claims
   `John/x.pdf` (no `\B/` anchor). A match never holds ` /` (the space before a slash ends it), so every later
   separator belongs to the same path. The separator is `\` for a Windows home or UNC path, `/` otherwise (there a `\`
   is a `{:?}` escape).
4. **A ` -> ` arrow, anywhere** (`{from} -> {to}`), and the ` -` a Windows path's match stops at before its `>`.
5. **A word that's already a token** (`…/<dir:…> in 3ms`, `…/<dir:…> $HOME/…`): this scanner's own output on a second
   pass. A segment the token leads ends after it; a token after a name ends the path before it. Only an exact token
   shape counts (`is_redacted_segment`), so `Lake <draft> House` stays one name.

**`<` and `>` are path characters** on the local branches (home, system, mount, `abs_path`, and the walk inside
external text) and the remote ones (`remote_url`, `unc`): both are legal in a macOS, Linux, or NAS name, and a tail
that stopped at one shipped the rest (`Lake <draft> House/beach.jpg` lost only `Lake`; pinned by
`angle_brackets_inside_a_name_never_end_its_path` and `angle_brackets_inside_a_remote_name_never_end_its_reference`).
So a token is a path chunk on a second pass, which is why every rewriter keeps tokens on scanner-owned input
(`preserve_redacted_segments`; for a URL that's each component, `<user:…>:<credential:…>` split after the first `>`),
rule 5 exists, and an unbalanced trailing `>` goes back like any closing bracket (`<https://…/x>`). `abs_path`'s
first segment still can't start with either, which leaves a URL's `//<user>:…@<host>/…` to the remote branches.
Windows forbids both, so `win_home` stops there.

After the cut, only characters with no name in them go back: `:<digits>` source locations, trailing `.` `,` `;` `!`
`?`, and a closing bracket the path didn't open (`(under …/Summer trip): disk full`, the `move` line). A balanced `)`
stays (`Summer (Lake House)`), and so does the `}` of a `\u{301}` escape. `(` and `)` never end a path.

**The cost, accepted until producers mark their values (#491):** prose after an unkeyed path goes with it, extension
and file kind included. `/Users/jo/Documents/notes.md failed to open` reads `$HOME/Documents/<dir>`, an extension in the
swallowed prose becomes the leaf's (`x.pdf for alice@example.com` reads `<file>.com`), and
`VolumeManager::report_identity_conflict`'s line loses its volume ID up to the `; ` (the 98 truncated reports this
section used to warn about). Pinned by `prose_after_an_unkeyed_path_goes_with_it`. A keyed field (`path={:?}`) keeps
everything after it; that's the fix for a line whose prose matters.

The same boundary serves every caller: the prose branches, an unquoted absolute `path_field` value, the leaf echo
scrub, and the absolute-path walk inside external text.

### Why the scan resumes mid-match

`redact_with` walks the line itself rather than calling `replace_all`, resuming at `match.start() + consumed` where
`consumed` is the path length without the noise. `replace_all` resumes after the WHOLE match, so every handed-back byte
was skipped by the scanner: `/Volumes/d/f.txt and smb://host/share/x.txt` pulled `smb:` into the volume match, gave it
back as literal text, and left `//host/share/x.txt` with no pattern willing to claim it. The share name and filename
shipped. Any future branch that hands text back needs the same treatment, which is why `dispatch` returns a length.

## Names in prose

Rust log sites interpolate paths in prose far more often than as keyed fields: about 370 `{}` of `path.display()` and
100 inline `{path}` captures as of 2026-10-06. Two report-side rules cover that class without touching the sites:

- **`abs_path` claims an absolute path under any prefix.** The prefix branches only knew home, the system temp roots,
  and mounts, so `/srv/clients/…`, a phone's `/DCIM/…`, or a server's `/data/…` shipped whole. The regex crate has no
  lookbehind; `\B` before the slash (no word char in front) is what keeps `MB/s`, `and/or`, `3/4`, and `$HOME/…` out,
  and the two-segment minimum keeps a lone `/` or `/foo` prose. It ends like every prose path (`split_trailing_noise`)
  and walks the segments with `redact_relative_path`, so a system root stays and an over-match like an unknown-scheme
  URL's path is tokenized rather than shipped. A word after the first segment may start with `(` (`/srv/A/Summer (Some
  Words) Name/…`); the first segment can't, so `(see /x/y)` doesn't start a path at its paren. Inside external text
  `redact_any_absolute_path` runs BEFORE the line scanner, as a typed path.
- **The leaf echo scrub** (`detail.rs::scrub_leaf_echoes`). macOS repeats a file's name in its own error text (`the
  Trash refused it: “Screenshot ….jpeg”`). `redact_with` collects the leaf of every local or absolute path on the line,
  with the token the path's own rewrite gave it (`echoed_identities`), and replaces whole-word repeats in the prose
  between matches, before or after the path. A leaf the rewrite kept (`Documents`) isn't scrubbed. Glue is stricter
  than inside external text: a letter, digit, `_`, or `::` joins a word, so a folder named `media` leaves the
  `media_index` log target alone.

What this doesn't cover: a bare name or a relative path whose line names no path for it (`couldn't open “x.pdf”` on
its own), and a leaf repeated after a remote URL. Those sites log the value under a key (`file={name:?}`,
`smb_path={p:?}`); the convention is in `CLAUDE.md`, and the frontend bridge does it automatically by placeholder
name.

## How to add a new pattern

1. Add a new alternative inside `redactor_regex()` with a unique `(?P<group_name>...)` and write a corresponding
   rewriter (or extend `dispatch`) to map matches to redacted output.
2. Add a dedicated test in `tests.rs` with at least six input→expected tuples covering edge cases (start of line, middle
   of line, embedded in punctuation, multiple per line).
3. Append two or three lines to `fixtures/log-corpus.txt` exercising the new pattern, and update
   `fixtures/log-corpus.redacted.txt` to match. The `replacement_count_histogram` test flags a corpus missing your
   pattern.

## The module layers

The rewriters import downward only: `names.rs`, `context.rs`, and three leaves (`tokens.rs`: what a redacted value
looks like, `is_redacted_segment` and `is_name_token`; `placeholders.rs`: the span pass's placeholders; `echoes.rs`:
`EchoedIdentity`) at the bottom, then `path_end.rs`, `paths.rs`, `references.rs`, `fields.rs`, and the two passes on
top (`markers.rs` and `detail.rs`, which reads `markers::Spans`). A helper every layer asks goes in a leaf, ❌ never in
the rewriter that first needed it: `is_redacted_segment` in `fields.rs` and `is_placeholder` in `markers.rs` welded six
modules into one cycle (`desktop-rust-module-cycles`, `scripts/check/checks/DETAILS.md` § "Rust module cycles").

## Regex and line-splitting notes

- `redact_text` splits on `\n` and redacts each line independently. This keeps regex `\b` anchors predictable and lets
  us return `Cow::Borrowed` per line.
- Verbose regex mode (`(?x)`) ignores whitespace OUTSIDE character classes. Inside `[...]` whitespace is literal, so
  `[A-Za-z]` is fine but `[ A-Za-z ]` would match a space.
- Paths with embedded spaces (`/Volumes/My Backup Drive/...`) match by allowing single spaces between path components.
  Multi-space gaps stop the match. Where the match then actually ends: § "Finding the end of a path".

## Keyed path fields (`path_field`)

A path with no mount prefix (`docs/a b.pdf` on an SMB share, `/docs` relative to a volume root, a bare `name.jpg`)
looks like any other word, so the only thing that can mark it is the key it's logged under. `path_field` claims a
fixed set of keys (see the regex) with either a `{:?}`-quoted value or a bare one:

- **Quoted** is exact: the value is unescaped (so `e\u{301}` is one character again and a `\` inside an escape isn't
  read as a separator), redacted as one complete typed value, and re-quoted. It never enters the prose-boundary scanner.
- **Bare** (`smb2`'s own `tree: renamed from=a\b c.jpg to=…`) over-matches to the end of the line, and
  `end_of_bare_value` cuts at the first `: ` seam, `, `, or ` key=`, then drops an unbalanced `)`. A comma-space inside
  a bare name ends it early and leaks the rest; `{:?}` values can't hit that, which is why our own sites use it.
- **An unquoted absolute value** that a path branch claims from its first byte is redacted as a typed path ending where
  `split_trailing_noise` says (`…/Medical records` stays one leaf and matches its quoted spelling's token).
- **A home role survives a second pass**: `path=$HOME/Pictures/<dir>, …` keeps `Pictures` (`home_role_len`), which is
  what keeps report redaction idempotent.
- **`{:?}`-printed structs** use `key: "…"` (`PermissionDenied { path: "…" }`); a quoted value after `: ` is the same
  complete typed value. `path: ` before anything else is prose and gets rescanned. Anything else is walked here by `redact_relative_path`:
  same leaf and allowlist rules, the first segment of an absolute value kept if it's a system root (`/private`,
  `/Applications`), and already-redacted segments left alone, which keeps it idempotent.
- The key set is deliberately narrow: `name=` stays out because it names hosts and settings too (`Host …: name=NAS`),
  and `target=` because the file viewer uses it for a seek target. A name-bearing site logs under `new_name=` instead.

## Producer-owned identity fields

Bare remote identities have no safe lexical shape. The redactor therefore claims only quoted values under exact typed
keys: `host`, `server`, `share`, `volumeId`, `volumeName`, `serverId`, and `deviceId` (`volumeName` shares the
volume domain with `share`; the frontend log bridge emits it). Values may use `Some("…")`. Near matches,
generic `name=` / `id=`, and unquoted legacy fields are excluded so ordinary diagnostics do not disappear. Producers
that own an identity must emit its Rust debug form under one of those keys (`host={host:?}`), never put literal quotes
around Display output. A value that's already a token (a second pass over a report line) stays, quoted or bare, `user=`
included. The same escape-aware quoted grammar applies to `user` / `username`. Arbitrary external prose
goes in an external-text field (next section), never in a free-form log message.

## External-text fields

OS, server, and CLI output (smbclient/smbutil/diskutil/gio stderr, an SFTP status sentence, an `smb2` or `reqwest`
error's `Display`) is often the only direct window into what went wrong, and it can repeat names with no path or key
around them. So it has one mechanism, used at every such site:

- **Producers log it whole** as `detail={:?}` (or `stderr={:?}` / `stdout={:?}`) through `cmdr_fs::log_detail::LogDetail`,
  which trims, caps at 1 KiB with a `…[N bytes]` marker, replaces any marker char in the text with U+FFFD, and
  Debug-quotes. Local logs keep it; producers never redact. `LogDetail(text).marking(&[…])` marks the values the caller
  handed the OS or tool where the text echoes them, so they hash exactly, one depth down (`crates/cmdr-fs/DETAILS.md` §
  "Marked log values").
  Next to it they log the machine-readable code wherever one exists as a typed field: `code=` (errno or exit status),
  `nt_status=` (SMB), `sftp_status=` (SFTP v3 wire number), `error_kind=`.
- **Redaction (`detail.rs`) treats the quoted value as one unit.** It unescapes it (so a `\"` around a quoted name
  can't split a path match and strand a bare quote), runs the ordinary scanner with the same context per line, then
  scrubs whole-word repeats of every identity the SAME line logs under a key (`host=`, `server=`, `share=`, `user=`,
  the IDs, a quoted `path=` leaf) or as a path's leaf, reusing that field's token. It caps the result at `REPORT_DETAIL_MAX_CHARS` (200)
  with a trailing `…` and re-escapes with `{:?}`, so the closing quote stays exact. Idempotent: a capped value sits at
  the limit. A detail nested in one (Flow B's note quotes a `log_error!` message that has its own `detail=`) gets the
  line's spans too, so it puts their tokens in before its own `{:?}` would escape the placeholders.
- **Identity-keyed JSON pairs inside the value are tokenized first** (`"server":"…"`, `"share"`, `"username"`, `"path"`,
  `"name"`, `"source"`, `"first"`, `"entry"`, `"deviceName"`, …): the frontend's `describeForLog` renders an object,
  and a typed error's payload, as compact JSON whose keys say what each value is, in a spelling the line's own keyed
  fields may not share. The backend's `WriteOperationError::log_detail` marks every field itself (`cmdr_fs::log_json`,
  a `message` through its `ErrorDetail`, `crates/cmdr-fs/DETAILS.md` § "Marked log values"), so its spans are read
  first and this pass never has to guess that `message` holds a path. The frontend marks those same values (`apps/desktop/src/lib/logging/DETAILS.md` § "Marking values for
  error reports"), so the span pass hashes them first; this is the fallback for a pair logged unmarked, and the two
  key sets stay in step (`fe_bridge_tests.rs`). `error`, `err`, `result`, and `detail` placeholders all render
  as `detail=` fields (`log-bridge.ts`).
- **Absolute paths under any prefix are tokenized inside the value** (`/srv/data/…`, `/mnt/…`), BEFORE the ordinary
  scan, as typed paths. A later word may start with `(` (`Path: …/Summer (Lake House) Trip/…; Error type: …`, a typed
  error the frontend flattened); the first segment can't. Already-rewritten segments keep their tokens, which keeps it
  idempotent.
- **The echo scrub reads the whole line** (collected once per line, lazily, in `redact_with`), so a key after the field
  still counts. External-text fields never feed it: prose can't teach it a name. Values under three chars are skipped
  (too likely to be part of a word), and matches glued to a letter or digit are left alone.
- **MCP's `cmdr://logs` gets the same treatment**, cap included, with bare tokens.
- **What it doesn't promise:** a name the line neither keys nor puts in a path survives in the prose (pinned by
  `an_unkeyed_bare_name_in_prose_survives`). The cap bounds exposure; it doesn't anonymize.

## Report-scoped token identity

`RedactionContext::for_report` derives a context key from a process-lifetime random 32-byte secret and the validated
report ID. Rebuilding preview and send for one ID in the same process reproduces tokens; another report ID or process
does not. `for_test` takes an explicit secret, so unit tests never replace global randomness. Tokens are the first six
SHA-256 bytes rendered as 12 lowercase hex characters. The hash input includes versioned labels and a domain tag to
separate path, host, userinfo, credential, query, fragment, volume, device, volume-ID, server-ID, and device-ID
identities. Every report surface uses the same context: the log line pass, the automatic note, the crash report's
strings, and the state history (whose typed names and paths go through `redact_name` / `redact_path`, so a cursor name
and the same leaf in a log line share a token).

Tokens are for spotting repeated normalized names, so identity sees through printing differences. `token` undoes
`{:?}` escapes and NFC-normalizes before hashing. Cmdr's `.cmdr-tmp-` / `.cmdr-temp-` / `.cmdr-staging-` suffix is split
off first (`split_cmdr_suffix`): the name part correlates with the final file and the suffix ships as-is, since its UUID
says nothing about anyone. A path token denotes a normalized segment name, not proof that two complete paths are equal.
