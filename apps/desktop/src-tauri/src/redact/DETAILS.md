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

## Pattern overlaps

- **Dispatch order mirrors the regex alternation order.** `remote_url` must claim recognized schemes before the generic
  userinfo branch, and IDs must be claimed before embedded address patterns can split them. Don't reorder without
  re-checking these overlaps.
- **`bare_userinfo` captures a leading delimiter (`^` or one whitespace) into `bare_lead` and re-emits it.** The regex
  crate has no lookbehind, so this anchoring is how the scheme-less `//user:pass@host` shape (built by the macOS
  `smbutil` / Linux `smbclient` fallbacks) avoids grabbing the `//user@host` tail inside a scheme'd `http://user@host`
  (handled by the earlier URL branches). Don't drop the lead capture.

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

Unsalted `redact_line` (ordinary MCP logs, operation summaries, listing failures, the crash hook) and
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
tokenize it again. Only a quoted path reached through `RedactionContext::redact_line` preserves existing generated
tokens, because that scanner can be reprocessing its own output and must remain idempotent. These methods are lexical
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
is a recognized identity and gets a report-local `<share>` token. A bare `share=` field still ships because the generic
field has no typed provenance yet. A bare NetBIOS name (`server=NASPOLYA`) is likewise a known remaining gap.

`None` passes through because it isn't a name, and the difference between "no username in the mount info" and "a
username we then looked up in the Keychain" is exactly what a triager reads these lines for. That's why this can't be a
blanket `user=\S+` rewrite.

## Finding the end of a path

A space can be inside a path (`/Volumes/My Backup Drive`), inside a filename (`Invoice for Acme Corp.pdf`), or the gap
between a path and the prose after it. Nothing local to the character tells the three apart, so the regex takes the
greedy option and `split_trailing_noise` hands back what wasn't path. Both directions of getting that wrong have shipped:

- **Too cautious** and the leftover is a filename fragment that goes out in a bundle. Anchoring continuation words to
  `[A-Z0-9]` stopped `Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg` at ` at`, so every multi-word filename leaked its
  tail. `Invoice for Acme Corp.pdf` is the same shape with something to lose.
- **Too greedy** and the line loses its message. A label group that swallowed following lowercase words truncated 98
  uploaded reports at `/Volumes/<volume>`, taking the volume ID and the resolution with it.

The boundary rules, in order, each earning its place against one of those:

1. **The `": "` seam.** Nearly every caller formats `{path}: {message}`. macOS forbids `:` in a filename, so cutting
   there can't shorten a real name; on Linux a `: ` inside one is vanishingly rare.
2. **Forward scan to the first token that ends the path**: one carrying a letter-led extension (`report.pdf`), or one
   ending a sentence (`naspi-1)`, `state.`). Forward, not backward, is the load-bearing part:
   `report.pdf for alice@example.com` has to cut after `report.pdf`, and from the right the email's `.com` is
   indistinguishable from a real extension.
3. **`ends_filename` demands a LETTER-led extension**, unlike the looser `has_extension_like_suffix` used for the
   `<dir>`/`<file>` decision. `01.13.03` in a timestamp otherwise reads as an extension and halves the name. A digit-led
   real extension (`.7z`) just doesn't end the scan, which costs a word of prose, never a leak.
4. **Backward trim of a lowercase extension-less run**, floored at the first segment after the last `/`. That floor is
   what leaves `/Volumes/naspi and then it failed` as `naspi` rather than nothing. It is skipped when the path runs
   right up to the seam, since the seam already marked the end: `/Volumes/x/summer trip: failed` used to ship `trip`. A
   word with an inner dot or a `{:?}` escape (`me\u{301}retek.jpg.cmdr-tmp-…`) also stops it: prose has neither, and
   trimming one shipped the tail of a temp name.
5. **A `\u{…}` escape is part of a name.** Its closing `}` neither ends a sentence nor gets trimmed as punctuation, so
   `cafe\u{301} menu.pdf` stays whole.

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
  URL's path is tokenized rather than shipped. Inside external text `redact_any_absolute_path` runs BEFORE the line
  scanner, because there a lowercase last word (`Medical records`) belongs to the path.
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
  `split_trailing_noise_with(value, false)` says, which is every rule except the lowercase prose run (the key already
  says it's a path, so `…/Medical records` stays one leaf and matches its quoted spelling's token).
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
around Display output. The same escape-aware quoted grammar applies to `user` / `username`. Arbitrary external prose
goes in an external-text field (next section), never in a free-form log message.

## External-text fields

OS, server, and CLI output (smbclient/smbutil/diskutil/gio stderr, an SFTP status sentence, an `smb2` or `reqwest`
error's `Display`) is often the only direct window into what went wrong, and it can repeat names with no path or key
around them. So it has one mechanism, used at every such site:

- **Producers log it whole** as `detail={:?}` (or `stderr={:?}` / `stdout={:?}`) through `cmdr_fs::log_detail::LogDetail`,
  which trims, caps at 1 KiB with a `…[N bytes]` marker, and Debug-quotes. Local logs keep it; producers never redact.
  Next to it they log the machine-readable code wherever one exists as a typed field: `code=` (errno or exit status),
  `nt_status=` (SMB), `sftp_status=` (SFTP v3 wire number), `error_kind=`.
- **Redaction (`detail.rs`) treats the quoted value as one unit.** It unescapes it (so a `\"` around a quoted name
  can't split a path match and strand a bare quote), runs the ordinary scanner with the same context per line, then
  scrubs whole-word repeats of every identity the SAME line logs under a key (`host=`, `server=`, `share=`, `user=`,
  the IDs, a quoted `path=` leaf) or as a path's leaf, reusing that field's token. It caps the result at `REPORT_DETAIL_MAX_CHARS` (200)
  with a trailing `…` and re-escapes with `{:?}`, so the closing quote stays exact. Idempotent: a capped value sits at
  the limit.
- **Identity-keyed JSON pairs inside the value are tokenized first** (`"server":"…"`, `"share"`, `"username"`, `"path"`,
  `"name"`, …): the frontend logs a typed error as `JSON.stringify(error)`, and its keys say what each value is, in a
  spelling the line's own keyed fields may not share. `error`, `err`, `result`, and `detail` placeholders all render
  as `detail=` fields (`log-bridge.ts`).
- **Absolute paths under any prefix are tokenized inside the value** (`/srv/data/…`, `/mnt/…`), BEFORE the ordinary
  scan, with no lowercase prose-run trim: the text is untrusted, so a trailing lowercase word goes with the path.
  Already-rewritten segments keep their tokens, which keeps it idempotent.
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

## Known gap: a lowercase last word before prose

`/Volumes/x/summer trip failed to open` still reads `trip failed to open` as prose, because nothing local tells a
folder's lowercase word from the sentence after it. The seam rule covers the common `{path}: {message}` shape; logging
the path as a quoted field covers the rest.
