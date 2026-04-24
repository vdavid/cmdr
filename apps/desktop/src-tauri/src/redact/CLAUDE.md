# Redact

Path-shape-preserving redactor used by the crash reporter and (Phase 4+) the error reporter.

The hot path is `redact_line`, called once per log line. One composed regex with named capture
groups drives a single pass; the dispatch closure inspects which group matched and calls the
matching rewriter. `Cow::Borrowed` is returned for lines with no matches so the no-PII case
costs zero allocations.

## Pattern table

| Group           | Matches                                              | Rewrites to                                             |
| --------------- | ---------------------------------------------------- | ------------------------------------------------------- |
| `unix_home`     | `/Users/<user>/...`, `/home/<user>/...`              | `$HOME/<allowlisted-parent-or-dir>/<file>.<ext>`        |
| `win_home`      | `C:\Users\<user>\...`                                | `$HOME\<allowlisted-parent-or-dir>\<file>.<ext>`        |
| `unix_system`   | `/tmp/...`, `/var/...`, `/private/...`, `/opt/...`   | Prefix kept; tail walked with same shape rules          |
| `volumes`       | `/Volumes/<label>/...` (label may contain spaces)    | `/Volumes/<volume>/<allowlisted-or-dir>/<file>.<ext>`   |
| `media`         | `/media/<label>/...` (label may contain spaces)      | `/media/<volume>/<allowlisted-or-dir>/<file>.<ext>`     |
| `smb_uri`       | `smb://host/share/...`                               | `smb://<host>/<share>/<redacted tail>`                  |
| `unc`           | `\\host\share\...`                                   | `\\<host>\<share>\<redacted tail>`                      |
| `url_userinfo`  | `scheme://user[:pass]@host/...`                      | `scheme://<userinfo>@host/...` (host preserved)         |
| `email`         | `local@domain.tld` (loose RFC 5321 ish)              | `<email>`                                               |
| `mdns`          | `<label>.local` bare hostnames                       | `<host>.local`                                          |
| `ipv4`          | dotted-quad with valid octet ranges                  | `<ipv4>`                                                |
| `ipv6`          | full + common compact forms (`::1`, `fe80::1`, ...)  | `<ipv6>`                                                |
| `mtp_owner`     | `<Owner>'s <Model>` MTP device names                 | `<mtp-owner>'s <Model>` (model phrase preserved)        |

### Path-shape preservation

For paths, we keep:

- The **mount/home prefix** as a fixed token (`$HOME`, `/Volumes/<volume>`, `/media/<volume>`,
  `/tmp/`, etc.).
- The **immediate parent directory name** if it's in the allowlist
  (`Documents`, `Downloads`, `Desktop`, `Library`, `src`, `Pictures`, `Movies`, `Music`,
  `Public`, `AppData`, `Application Support`).
- The **file extension** if it's <= 8 ASCII alphanumeric chars.

Everything else collapses to `<dir>` or `<file>`. So
`/Users/john/Documents/budget.pdf` → `$HOME/Documents/<file>.pdf`, but
`/Users/john/SecretProject/budget.pdf` → `$HOME/<dir>/<file>.pdf`.

### Decision: why path-shape preservation + allowlist

Tradeoff between debuggability ("I can see this is a Documents path") and PII safety
("but I don't want to leak project codenames"). The allowlist captures the dirs that are
near-universal across users — anything custom collapses. Net result: triagers can usually
guess the failure context without seeing the user's secrets.

### Decision: MTP owner names redacted, model names kept

Phase 1 punted on MTP names; a post-implementation fix added an `mtp_owner` pattern that
catches the common shape `<Owner>'s <Model>` (e.g., `John's Pixel 8 Pro`). The owner part
becomes `<mtp-owner>` and the model phrase (`Pixel 8 Pro`, `iPhone 15 Pro`, ...) is kept
because model strings alone aren't identifying and are useful diagnostic context.

The pattern requires both a capitalized possessive AND a model word from a known set
(`iPhone | iPad | Pixel | Galaxy | OnePlus | Note | Tablet | Phone | Camera | ...`)
immediately after the `'s `. This keeps English contractions (`it's a Pixel`) and module
paths (`cmdr_lib::mtp::device`) untouched. `That's Pixel 8 Pro` does match — accepted as
an over-redaction (rare phrasing without an article between `'s` and the model word).
Bare model names like `Pixel 8 Pro` are deliberately NOT redacted.

## How to add a new pattern

Three steps:

1. Add a new alternative inside `redactor_regex()` with a unique `(?P<group_name>...)` and
   write a corresponding rewriter (or extend `dispatch`) to map matches to redacted output.
2. Add a dedicated test in `tests.rs` with at least 6 input→expected tuples covering edge
   cases (start of line, middle of line, embedded in punctuation, multiple per line).
3. Append two or three lines to `fixtures/log-corpus.txt` exercising the new pattern.
   Update `fixtures/log-corpus.redacted.txt` to match. The `replacement_count_histogram`
   test will tell you if the corpus is missing your pattern.

## Files

| File                                | Purpose                                                       |
| ----------------------------------- | ------------------------------------------------------------- |
| `mod.rs`                            | Public API + composed regex + path rewriters                  |
| `tests.rs`                          | Per-pattern tests, idempotency, golden corpus, histogram      |
| `fixtures/log-corpus.txt`           | Synthesized log lines covering every pattern class            |
| `fixtures/log-corpus.redacted.txt`  | Expected redaction of the corpus (golden snapshot)            |

## Gotchas

- The dispatch order in `dispatch()` mirrors the alternation order in the regex. SMB URIs
  with userinfo (`smb://user@host/...`) match `smb_uri` first (it's listed earlier) — they
  do **not** fall through to `url_userinfo`. The userinfo is dropped along with the host.
- `redact_text` splits on `\n` and redacts each line independently. This keeps regex `\b`
  anchors predictable and lets us return `Cow::Borrowed` per line.
- Verbose regex mode (`(?x)`) ignores whitespace **outside** character classes. Inside
  `[...]` whitespace is literal, so `[A-Za-z]` is fine but `[ A-Za-z ]` would match a space.
- Paths with embedded spaces like `/Volumes/My Backup Drive/...` are matched by allowing
  single spaces between path components. Multi-space gaps stop the match.
- The `url_userinfo` pattern preserves the host on purpose — the assumption is that the host
  is part of a well-known service URL the developer needs to see. If we ever store private
  hosts in URLs, revisit this.
