# Redact

Path-shape-preserving redactor shared by the crash reporter and the error reporter.

The hot path is `redact_line`, called once per log line: one composed regex with named capture groups, single pass,
dispatch closure inspects the matched group and calls its rewriter. `Cow::Borrowed` for no-match lines (zero alloc).
`redact_line_salted(line, &salt)` is the same pipeline with a per-bundle salt: segments that would collapse to `<dir>` /
`<file>` instead emit `<dir:HHHHHH>` / `<file:HHHHHH>` (`sha256(salt || segment)[..3]`), so equal segments correlate
within one bundle but not across bundles. The builder mints a fresh 16-byte random salt per build; the salt never ships.

## Pattern table

| Group | Matches | Rewrites to |
| --- | --- | --- |
| `unix_home` | `/Users/<user>/...`, `/home/<user>/...` | `$HOME/<allowlisted-parent-or-dir>/<file>.<ext>` |
| `win_home` | `C:\Users\<user>\...` | `$HOME\<allowlisted-parent-or-dir>\<file>.<ext>` |
| `unix_system` | `/tmp/`, `/var/`, `/private/`, `/opt/` | prefix kept; tail walked with same shape rules |
| `volumes` | `/Volumes/<label>/...` (spaces allowed) | `/Volumes/<volume>/<allowlisted-or-dir>/<file>.<ext>` |
| `media` | `/media/<label>/...` (spaces allowed) | `/media/<volume>/<allowlisted-or-dir>/<file>.<ext>` |
| `smb_uri` | `smb://host/share/...` | `smb://<host>/<share>/<redacted tail>` |
| `unc` | `\\host\share\...` | `\\<host>\<share>\<redacted tail>` |
| `url_userinfo` | `scheme://user[:pass]@host/...` | `scheme://<userinfo>@host/...` (host kept) |
| `bare_userinfo` | `//user[:pass]@host/...` (no scheme) | `//<userinfo>@host/...` (host kept) |
| `email` | `local@domain.tld` | `<email>` |
| `mdns` | `<label>.local` | `<host>.local` |
| `ipv4` | dotted-quad, valid octet ranges | `<ipv4>` |
| `ipv6` | full + compact forms (`::1`, `fe80::1`) | `<ipv6>` |
| `mtp_owner` | `<Owner>'s <Model>` device names | `<mtp-owner>'s <Model>` (model kept) |

## Must-knows

- **Path-shape preservation keeps three things, collapses the rest.** The mount/home prefix as a fixed token (`$HOME`,
  `/Volumes/<volume>`, `/tmp/`, etc.); the immediate parent dir name IF allowlisted (`Documents`, `Downloads`,
  `Desktop`, `Library`, `src`, `Pictures`, `Movies`, `Music`, `Public`, `AppData`, `Application Support`); and the
  extension if ≤ 8 ASCII alnum chars. So `/Users/john/Documents/budget.pdf` → `$HOME/Documents/<file>.pdf`, but
  `/Users/john/SecretProject/budget.pdf` → `$HOME/<dir>/<file>.pdf`.
- **Leaf `<dir>` vs `<file>` is decided by `has_extension_like_suffix`** (`.X`, 1-8 alnum, dot not at position 0). So
  `notes.md` → `<file>.md` but `Application Support` → `<dir>`. Trade-off: an extensionless file (`id_rsa`, `README`,
  `Makefile`) is mislabeled `<dir>`. Accepted: Cmdr logs are dominated by directory listings, so `<dir>` reads more
  accurately on real triage data.
- **MTP owner redacted, model kept.** `mtp_owner` requires a capitalized possessive AND a known model word
  (`iPhone | iPad | Pixel | Galaxy | OnePlus | ...`) right after `'s `, leaving contractions (`it's a Pixel`) and module
  paths untouched. Bare model names (`Pixel 8 Pro`) are NOT redacted (not identifying, useful diagnostics).

## Gotchas

- **Dispatch order mirrors the regex alternation order.** `smb://user@host/...` matches `smb_uri` first (listed
  earlier), so it does NOT fall through to `url_userinfo`; the userinfo is dropped with the host. Don't reorder without
  re-checking these overlaps.
- **`bare_userinfo` captures a leading delimiter (`^` or one whitespace) into `bare_lead` and re-emits it.** The regex
  crate has no lookbehind, so this anchoring is how the scheme-less `//user:pass@host` shape (built by the macOS
  `smbutil` / Linux `smbclient` fallbacks) avoids grabbing the `//user@host` tail inside a scheme'd `http://user@host`
  (handled by the earlier `url_userinfo`). Don't drop the lead capture.
- **`url_userinfo` preserves the host on purpose** (assumed to be a well-known service URL the dev needs). Revisit if we
  ever store private hosts in URLs.

## Files

`mod.rs` (public API + composed regex + rewriters), `tests.rs` (per-pattern, idempotency, golden corpus, histogram),
`fixtures/log-corpus.txt` + `.redacted.txt` (golden snapshot).

Full details (decision rationale, how to add a pattern, regex verbose-mode notes): [DETAILS.md](DETAILS.md).
