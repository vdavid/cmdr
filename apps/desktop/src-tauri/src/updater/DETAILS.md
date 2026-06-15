# Updater module — details

Read before structural changes. `CLAUDE.md` holds the must-knows that prevent silent breakage; this is the depth.

## Key decisions

- **Sync files into the bundle instead of replacing the `.app` directory.** Replacing changes the inode, which makes
  macOS TCC lose FDA grants. (Guardrail in `CLAUDE.md`.)
- **Sync order: Resources, Info.plist, _CodeSignature, then the MacOS binary last.** Updating the binary last minimizes
  the window where the code signature is inconsistent with the binary on disk; if the app crashes mid-update, the old
  binary is still intact.
- **Unconditional deletion of stale files after sync.** Old files left behind could cause version mismatches or bloat.
  The deletion pass removes anything in the destination not in the source, then cleans empty directories bottom-up.
- **Minisign verification before writing the tarball to disk.** Ensures integrity and authenticity; the public key is
  compiled into the binary. Both key and signatures use base64(minisign-text-format), matching Tauri's convention.
- **Privilege escalation via `osascript` with `rsync -a --delete`.** When installed in `/Applications` (root-owned),
  direct writes fail; `osascript`'s `do shell script … with administrator privileges` shows the native auth dialog.
  `rsync` expresses the full sync (copy + delete stale) in one shell command. Only triggers when direct writes are
  denied, so users running from `~/Applications` or a dev build won't see the dialog.
- **Atomic rename instead of in-place `fs::copy`.** (Inode / code-signing-cache rationale is in `CLAUDE.md`.)
- **Bounded manifest-fetch timeouts.** `reqwest::get`'s default client has no overall timeout; a stuck TCP handshake to
  the redirect target was observed hanging ~2.5 min, which made transient network blips look like a hung app and tripped
  the auto error reporter. Download/install stay untimed (user attention; can legitimately take a while).
- **Walk `reqwest::Error::source()` for log-friendly messages (`describe_error_chain`).** `reqwest::Error`'s `Display`
  only prints the outermost layer, hiding the real cause (DNS, TCP connect timeout, TLS). Walking the source chain
  surfaces the underlying class without pulling in `anyhow`.

## Dependencies

- `reqwest`: HTTP client for manifest + tarball download.
- `minisign-verify`: signature verification.
- `flate2`, `tar`: tarball extraction.
- `filetime`: touches the bundle after install to trigger a LaunchServices refresh.
- `base64`: decodes the double-encoded minisign key and signatures.
