# cmdr-update-trust: details

Read this before changing the check, its requirement, the extraction, or either caller.

## Why the check exists

Minisign proves an update archive came from whoever holds the updater key; it says nothing about what's inside. So
before the macOS updater writes anything into the running bundle, it asks `verify_release_bundle` whether the extracted
`Cmdr.app` carries a valid Apple code signature from Cmdr's Developer ID. A leaked updater key on its own can then only
produce archives that every install refuses: the attacker would also need the Developer ID signing identity
(Apple-issued, revocable by Apple) to ship anything. The updater's side (where it runs, what it answers):
`apps/desktop/src-tauri/src/updater/DETAILS.md` § Code signature.

## The check

- **The API.** Security.framework (`SecStaticCodeCreateWithPath`, `SecRequirementCreateWithString`,
  `SecStaticCodeCheckValidity`) through the `security-framework` crate, ❌ not a `codesign` subprocess: no parsing of
  output, and the refusal comes back as an `OSStatus` that maps to a typed `CodeSignatureRefusal` (`Unsigned`,
  `NotOurs`, `Invalid { status }`). Flags: all architectures, `kSecCSStrictValidate`, nested code, restricted symlinks.
  That's `codesign --verify --strict --deep`, so a changed, added, or removed sealed file fails.
- **The requirement** is `RELEASE_REQUIREMENT`, byte for byte the designated requirement every release already carries:
  identifier `com.veszelovszki.cmdr`, Apple-anchored, the Developer ID intermediate and leaf markers, and
  `certificate leaf[subject.OU] = "83H6YAQMNP"`. The team pin is the load-bearing part, since Apple issues Developer ID
  certificates to anyone. The comment on the constant explains each clause. A certificate renewal keeps the team ID;
  changing the bundle identifier or the team would stop every update (and every FDA grant).
- **The extraction** is `extract_tarball`, the `tar` crate the installer uses. It drops xattrs and sets its own modes,
  so the gate has to unpack the same way: the system `tar` keeps xattrs and could pass a bundle the installer's copy
  fails.
- **Independent of the minisign key**: the check runs the same whichever trusted updater key verified the archive, so a
  backup-signed rotation release must be a Developer ID build too.
- **Cost.** Under a second for a full release bundle (verified on macOS 27.0, `codesign --verify --strict --deep` on
  v0.52.0 aarch64, 2026-10-15).

## Why there's no switch

There's no runtime switch at all: not a manifest field, env var, setting, or build flag. Considered and rejected:
"enforce only when the running app is itself team-signed". It buys nothing, because what's being checked is the
DOWNLOADED bundle, and every real update is a published release signed by the team, so a dev or locally built copy that
reaches the install path passes the check like a shipped one. And it would add a fail-open branch that a broken
self-check (say, a running bundle already rewritten by an earlier sync) could silently take. Tests inject the check
instead: `archive::vet_with` here, `vet_staged_bundle`'s `code_signature` parameter in the app.

## The release gate (`cmdr-vet-update`)

`vet_update_archive(archive, version)` unpacks an archive into a temp dir with `extract_tarball`, requires `Cmdr.app` at
its top, runs `verify_release_bundle` on it (first, as the installer does: until it holds, the `Info.plist` is only what
the signer wrote), then requires its `CFBundleShortVersionString` to equal the release's version. The binary runs it on
every archive given and exits 1 if any is refused.

Why the version check: the app refuses a bundle that isn't newer than itself (`refuse_unless_newer`), so an archive
naming the wrong version would either fail every install or, named lower, roll nobody forward. Equality with the release
is the strict form of that.

Two callers, so a refused archive can't reach `latest.json` by either path:

- `scripts/release-finish` (`vet.go`) runs it on the downloaded draft archives after the provenance check and before
  signing, in real runs and dry runs.
- The release pipeline's `vet-update-archives` job (macOS) runs it on the release's archives in whichever run publishes
  (a `ci` mode tag push, or the `local` finishing dispatch), and `publish` needs it. A `local` tag push skips it, since
  the laptop vets before signing.

How it fits the release: `docs/guides/releasing.md` § The release gate.

## Verified against real releases

The opt-in test `archive::tests::a_published_release_passes_and_its_doctored_copies_dont` extracts a published archive
with `extract_tarball` and asserts the genuine bundle passes (and `vet_update_archive` at its own version), an ad-hoc
`codesign --force --deep -s -` copy is `NotOurs`, and a copy with one changed byte in `Info.plist` is `Invalid`. It
passed for the v0.52.0 aarch64, x64, and universal archives (macOS 27.0, 2026-10-15). Re-run it before changing the
requirement, the flags, or the extraction:

```sh
gh release download v0.52.0 -R getcmdr/cmdr -p 'Cmdr_0.52.0_aarch64.app.tar.gz' -D <scratch>
CMDR_UPDATER_RELEASE_TARBALL=<scratch>/Cmdr_0.52.0_aarch64.app.tar.gz \
  cargo test -p cmdr-update-trust --lib -- --ignored a_published_release_passes_and_its_doctored_copies_dont
```

(`cargo test` rather than nextest: the universal archive runs past nextest's slow-test timeout.) The always-on unit
tests build an ad-hoc signed fixture with `codesign` at test time (`fixtures.rs`) and cover unsigned, ad-hoc,
Apple-signed-but-not-Cmdr, a changed resource, an added resource, a changed executable, and for the gate: no `Cmdr.app`,
not an archive, the wrong version, and the signature checked before the version.

The full update path, from a pipeline-built archive through an installed app's real update: the proof record in
`docs/notes/2026-10-15-update-path-proof.md`.
