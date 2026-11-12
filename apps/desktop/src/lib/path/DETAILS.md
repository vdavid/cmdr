# Path module details

Depth and rationale. `CLAUDE.md` holds the must-knows that prevent silent breakage; this file holds the why.

## Why the `CanonicalPath` brand exists

`FilePane.svelte`'s `currentPath` is sometimes the literal string `"~"`: it's the default tab path and the home-shortcut
target, and the backend re-expands it on every IPC. Pressing Enter on the `..` row from `~` used to land at `/` because
`"~".lastIndexOf('/')` is `-1` and the fallback was `/`. Backspace had the same bug, and so did the `currentFolderName`
derivation (`"~".split('/').pop() === "~"`). Branding makes those derivations impossible without first canonicalizing.

## Why the brand is local, not viral

Only the few path-arithmetic operations (`parentOf`, `basenameOf`) require the brand; every other path-typed variable
stays `string`. `CanonicalPath` is assignable to `string`, so branded values flow outward freely; `string` is not
assignable to `CanonicalPath`, so the conversion has to be explicit. No `CanonicalPath | string` unions anywhere. This
keeps the brand from spreading into every signature while still gating the one operation that's actually unsafe.

## Virtual-volume URLs use slash arithmetic too

`mtp://device/storage/Music` becomes `mtp://device/storage` via the same `lastIndexOf('/')`. The brand exists to make
this safe to assume: anything that survived `toCanonical` is known to have a slash where the parent boundary lives.

## Canonical is not OS-resolvable

`CanonicalPath` answers "is slash arithmetic safe here", and that is all. `mtp://dev/65537/Music` passes, because the
parent boundary is where a `lastIndexOf('/')` says it is; nothing outside Cmdr can open it. The two questions look like
one and are not, so the outbound guard is a separate export rather than a stricter brand: paths flow through the app as
plain strings and only a handful of call sites hand one to an OS API.

## Volume membership is a component match

`isPathOnVolume(path, volumePath)` lives here rather than beside its first caller because three unrelated areas ask the
same question: pane navigation drops a path that doesn't belong to the volume it's pinned to, reveal-in-pane picks the
pane that can actually show a file, and the transfer dialog derives its volume-relative destination.

Two shapes make the naive `path.startsWith(volumePath)` wrong, and both reach real users:

- **A sibling root sharing a prefix.** `/Volumes/testshare` would claim `/Volumes/testshare-backup`, and a remote volume
  rooted at `/srv/data` would claim `/srv/data-1`. Appending the separator before comparing is what makes it a component
  match. `cmdr-fs`'s `RemoteRoot::to_remote_path` defends the same boundary on the Rust side, for the same reason.
- **A root that already ends in a slash.** A remote volume is rooted at `<prefix><server-side root>`, so one rooted at
  `/` spells itself `sftp://test_user_1@testnas.local:22/`. That is the DEFAULT for SFTP, WebDAV, and ADB, not an
  oddity.

The second shape is why `toVolumeRelativePath` trims the root before slicing. It used to slice by raw length, which ate
the separator and prefilled the copy destination with `home/test_user_1/…`; `validateDirectoryPath` rejected it as
relative and `handleConfirm` refused to dispatch, so Copy silently did nothing on every SFTP server rooted at `/`. A
volume rooted at a subfolder carries no trailing slash and worked fine, which made it look intermittent.

`isPlainFilesystemPath` is that guard. It answers false for every virtual-volume URL and for `~`-rooted and relative
paths, since none of them resolves without a base the OS API lacks. Its live caller is the search-results clipboard
refusal (`file-explorer/pane/clipboard-operations.ts::snapshotClipboardRefusal`), where a snapshot row can name a file
on an MTP storage or an ADB device: `NSURL::fileURLWithPath` reads an unknown scheme as a RELATIVE path and returns a
file URL under the process working directory, so a missing check ships a mangled path rather than a refusal.

## The app-path grammar

`grammar.ts` holds it, and its module doc is the canonical statement: an app path is a scheme URL, or a `/`-separated
absolute path whose first component is the root. The roots are `/` on macOS and Linux, and on Windows a drive (`C:/`,
letter uppercased) or a UNC share (`//server/share/`, ONE root component). Each drive and share is its own volume, so a
path names its volume the way a remote volume's `<scheme>://` path already does (`cmdr-fs`'s `remote_paths.rs`). The
decision and the rejected alternatives (volume-relative `/Users/test_user_1` plus a volume id, native `\` on the wire,
MSYS-style `/c/Users`) are D1 in `docs/specs/windows-port-plan.md`.

Choices the grammar makes, each pinned by a row in `crates/cmdr-fs/src/volume/path_grammar_cases.json`:

- **The flavor is the LOCAL backend's, so it follows the platform.** `C:/Users` is a relative POSIX path on macOS and
  Linux, where it stays unknown (a drive-shaped path typed there is rejected as relative, as before), and
  `//server/share` is POSIX `/server/share`. On Windows a bare `/x` is no app path, and neither is anything holding a
  `\`.
- **A root has no parent inside its volume.** `parentInVolume` answers `null` there; `parentOf` / `parentOrRoot` keep
  the old convention, a root being its own parent (`/` → `/`, `C:/` → `C:/`). What "up" from `C:/` shows is the pane's
  question, not the grammar's.
- **`.` and `..` resolve lexically, clamped at the root**, as `remote_paths.rs` does on the Rust side. No `stat`.
- **`//?/` and `//./` are refused**: they're Win32's device namespaces, not shares.
- **A scheme URL keeps plain slash arithmetic.** Where its root sits is its volume's business, so the grammar answers
  `null` for its root, root-ness, and segments.

## Showing and typing a path

`shownPath` is the one formatter from a wire path to what a person reads: `C:\Users\test_user_1`, `\\server\share\x` (a
share's root without its trailing `\`), and every other text unchanged. It reaches the screen through three doors:

- `#lib/utils/display-path`'s `displayPath` (every shown path: the breadcrumb readout, tooltips, dialogs, toasts, the
  operation log, `useShortenMiddle`), and `copiedPath`, so Copy path pastes `C:\Users\test_user_1` on Windows.
- `t()` runs it over each param its caller marked with `pathParam(…)` (`intl/messages.svelte.ts`), and over nothing
  else: a formatted message is prose, where a `/` may be "and/or", and a guess read a drive's name (`C:`) as `C:\` and
  an `a//b` as a share (#1118). An unmarked path param reads with `/` on Windows, which is honest; a guessed one mangled
  text. Off Windows it's a no-op.
- The breadcrumb's separator glyph and its segment tooltips (`shownSeparator`).

The other direction is `fromUserInput`: on Windows both `\` and `/` separate and a local path comes out normalized
(`c:\Users\` → `C:/Users`); on macOS and Linux the text is untouched, since `\` is a legal filename character there. Go
to path's resolver call and `validateDirectoryPath` use it. `typedSeparators` is its first half alone (separators
turned, nothing normalized), for the transfer destination, where a trailing separator means "into this folder".
`validateShareNamed` refuses `\\server` with no share there, which the grammar reads as no path at all.

## Case sensitivity is the volume's

`VolumeInfo.caseSensitive` comes from Rust (`LocationInfo.case_sensitive`, answered per OS arm behind
`volumes/platform/`: `apps/desktop/src-tauri/src/volumes/DETAILS.md` § "Case sensitivity"). `isPathOnVolume`,
`pathRelativeTo`, and `samePath` take it as `{ caseSensitive }` and fold case (and Unicode form) only when it says
`false`; without one they compare exactly.

No call site passes it yet, on purpose. Every frontend path comparison today compares two spellings the BACKEND produced
(a listing, a tab, a favorite, a volume root), and the backend spells one file one way, so the exact compare is right on
any volume. Where a person's typing meets a listing, the comparison is case-insensitive by design on every volume:
type-to-jump (a search) and `validateConflict` (a case-only twin is worth a warning even where it's legal). The one
typed-text-meets-volume site is the transfer dialog picking a destination volume (`transfer-target.ts`). It reads a
typed path through the grammar (`typedSeparators`, `parsePath`): a drive or share path is always this machine's and
keeps its root through the split, so `D:\backup\x` lands on `D:/` (#1118). A Windows local volume is a drive or share
ROOT, and the grammar uppercases the drive letter, so membership there needs no fold. A volume mounted at a FOLDER on a
case-insensitive disk would: its whole chain (`isPathOnVolume`, `toVolumeRelativePath`, `anchorOnVolume`) has to fold
alike, or it picks a volume and then fails to slice its root off. That waits for such a volume to exist on Windows.
