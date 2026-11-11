# Path module

Frontend path arithmetic, safe regardless of where a path came from (raw IPC return, persisted tab state, `~`-rooted UI
shortcut, virtual-volume URL).

## Module map

- `grammar.ts`: the app-path grammar, **a scheme URL, or a `/`-separated absolute path whose first component is the
  root** (`/`; on Windows `C:/` and `//server/share/`), and its total helpers: `parsePath`, `normalizePath`, `rootOf`,
  `isVolumeRoot`, `parentInVolume` / `parentOrRoot`, `basenameInVolume` / `lastName`, `joinPath`, `pathRelativeTo`,
  `samePath`, `fromUserInput`, and `shownPath`, the ONE display formatter (`\` for a Windows local path).
- `canonical.ts`: the `CanonicalPath` brand plus `toCanonical` / `parentOf` / `basenameOf`, the separate
  `isPlainFilesystemPath` outbound-path guard, and the `isPathOnVolume` membership test.

## Must-knows

- **One grammar, two languages.** Rust's twin is `cmdr_fs::volume::path_grammar`; both suites read
  `crates/cmdr-fs/src/volume/path_grammar_cases.json`, so a new case goes there, ❌ never into one suite. The flavor
  (`posix` or `windows`) follows the injected platform; case sensitivity follows the VOLUME
  (`VolumeInfo.caseSensitive`), ❌ never the OS. On macOS and Linux every helper answers exactly what the slash
  arithmetic did.
- **❌ No raw `/` arithmetic on a path-named value** (`cmdr/no-raw-path-arithmetic`): on `C:/Users` a `lastIndexOf('/')`
  cuts the drive off. Opt out per line, with a reason, where the `/` is a protocol's.
- **A value that may be `~` goes through `CanonicalPath`** (`parentOf` / `basenameOf`): `"~"` is a real `currentPath`
  value (default tab + home shortcut), and `"~".lastIndexOf('/')` is `-1`, so an unbranded `..` lands at `/`. A path the
  backend spelled may use `parentOrRoot` / `lastName` directly.
- **Construct only via `toCanonical(raw, homeDir)`; never cast.** The brand admits absolute POSIX paths and
  virtual-volume URLs (`mtp://`, `smb://`, `search-results://`), and excludes `~`-rooted, relative, and other unsafe
  inputs. It's assignable to `string` (flows outward freely) but not vice versa (conversion must be explicit). The brand
  is local: keep every other path-typed variable a plain `string`, no `CanonicalPath | string` unions.
- **A canonical path is NOT an OS-resolvable one.** The brand admits virtual-volume URLs, so before handing a path to
  anything that expects a real file (the system clipboard's `NSURL::fileURLWithPath`, a drag-out promise), ask
  `isPlainFilesystemPath`. An unknown scheme is read as a RELATIVE path there and comes back as a file URL under the
  process working directory, silently. `DETAILS.md` § "Canonical is not OS-resolvable".
- **Ask `isPathOnVolume` whether a path belongs to a volume; ❌ never a raw `startsWith`.** It matches whole COMPONENTS,
  so `/Volumes/naspi` can't claim `/Volumes/naspi-backup`, and it tolerates the trailing slash a remote volume rooted at
  `/` genuinely carries (`sftp://ada@nas.local:22/`). ❌ Never slice a volume prefix off by raw length either: that eats
  the separator on such a root and yields a RELATIVE path, which every absolute-path check downstream then rejects.
  `DETAILS.md` § "Volume membership is a component match".
- **`toCanonical` throws on empty `homeDir`.** `FilePane.svelte`'s `userHomePath` is fetched async on mount and starts
  `''`. The pane-level `canonicalPath` `$derived` returns `null` while it's empty, so reactive callers must guard on
  `canonicalPath !== null` rather than catch the throw.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
