# Updater module — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the must-knows that prevent silent breakage; this is the depth.

## Key decisions

- **Sync files into the bundle instead of replacing the `.app` directory.** ❌ Not because replacing would cost the FDA
  grant: it wouldn't. TCC's `access` table has no path or inode column, and the stored requirement for Cmdr is
  `identifier "com.veszelovszki.cmdr" and anchor apple generic and … certificate leaf[subject.OU] = "83H6YAQMNP"`, so a
  grant follows the signature, not the bundle on disk (verified on macOS 26.5.2, TCC.db inspection plus a
  launch/move/relaunch of a signed probe, 2026-08-25; `docs/notes/self-move-to-applications-2026-08-25.md`). What the
  decision actually rests on: `com.apple.macl` on the bundle is per-file and IS lost when the directory is recreated,
  and the per-file atomic rename below needs a bundle to sync into. Both hold. Only the FDA half of the old rationale
  was wrong, so don't reach for "it would cost FDA" when defending this.
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
- **Check the HTTP status before parsing the manifest.** `response.json()` on a 5xx or an HTML maintenance page
  deserializes into a parse failure, which reads as "the manifest is malformed" and sends the reader to the wrong
  layer: the manifest is fine, the server didn't serve it. The status arm says so in its own words and names the code.
- **Walk `reqwest::Error::source()` for log-friendly messages (`describe_error_chain`).** `reqwest::Error`'s `Display`
  only prints the outermost layer, hiding the real cause (DNS, TCP connect timeout, TLS). Walking the source chain
  surfaces the underlying class without pulling in `anyhow`.

## A bundle that can't be written

Two macOS arrangements make the running `.app` unwritable, and the app can't fix either from inside:

- **App Translocation.** Opening Cmdr straight from where it was downloaded makes Gatekeeper run it from a randomized
  read-only mount under `/private/var/folders/…/AppTranslocation/`, rather than from its real path.
- **A mounted disk image.** The `.dmg` is still open and the app was launched from inside it.

Both fail writes with `EROFS`, which is `io::ErrorKind::ReadOnlyFilesystem`, NOT `PermissionDenied`. That matters twice
over: the escalate-to-admin arm never fired for them (so the install hard-failed with nothing said), and escalating
would not have helped anyway, since a read-only mount refuses root too. The signature is an install that keeps sending
update checks and never changes version, which is exactly the straggler shape the update dashboard couldn't explain.

`bundle_location::classify` answers with a `BundleWriteBlocker`, using two probes:

- `SecTranslocateIsTranslocatedURL` (Security.framework, macOS 10.12+) for the translocation case. The alternative,
  testing the path for `/AppTranslocation/`, is a private layout Apple never promised and whose break we'd never notice.
- `statfs`'s `MNT_RDONLY` as the catch-all, which covers a disk image, a read-only share, and translocation itself.

Translocation is reported in preference to the read-only volume it implies, so the log names the outer cause. Every
failure path answers "no blocker": a false negative costs a doomed download, while a false positive would stop updates
that would have worked.

Two callers gate on it. The frontend asks (`update_write_blocker`) once a check finds an update and before the
download, which is what stops an install pulling ~63 MB it can't apply once per poll interval forever, and raises the
"move Cmdr to Applications" dialog instead. `installer::install` asks again before extracting, so a direct caller can't
skip the gate; and `sync_bundle`'s `ReadOnlyFilesystem` arm answers the nested case without raising an admin prompt.

## Dependencies

- `reqwest`: HTTP client for manifest + tarball download.
- `minisign-verify`: signature verification.
- `flate2`, `tar`: tarball extraction.
- `filetime`: touches the bundle after install to trigger a LaunchServices refresh.
- `base64`: decodes the double-encoded minisign key and signatures.
