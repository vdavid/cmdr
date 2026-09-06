# Path module

Frontend path arithmetic, safe regardless of where a path came from (raw IPC return, persisted tab state, `~`-rooted UI
shortcut, virtual-volume URL).

## Module map

- `canonical.ts`: the `CanonicalPath` brand plus `toCanonical` / `parentOf` / `basenameOf`, and the separate
  `isPlainFilesystemPath` outbound-path guard.

## Must-knows

- **Do path arithmetic only through `CanonicalPath`.** `parentOf` / `basenameOf` take the brand and are the only blessed
  way to derive parent/basename in the frontend. Naïve `lastIndexOf('/')` on a raw path is the bug the brand exists to
  prevent: `"~"` is a real `currentPath` value (default tab + home shortcut), and `"~".lastIndexOf('/')` is `-1`, so an
  unbranded `..` derivation silently lands at `/`.
- **Construct only via `toCanonical(raw, homeDir)`; never cast.** The brand admits absolute POSIX paths and
  virtual-volume URLs (`mtp://`, `smb://`, `search-results://`), and excludes `~`-rooted, relative, and other unsafe
  inputs. It's assignable to `string` (flows outward freely) but not vice versa (conversion must be explicit). The brand
  is local: keep every other path-typed variable a plain `string`, no `CanonicalPath | string` unions.
- **A canonical path is NOT an OS-resolvable one.** The brand admits virtual-volume URLs, so before handing a path to
  anything that expects a real file (the system clipboard's `NSURL::fileURLWithPath`, a drag-out promise), ask
  `isPlainFilesystemPath`. An unknown scheme is read as a RELATIVE path there and comes back as a file URL under the
  process working directory, silently. `DETAILS.md` § "Canonical is not OS-resolvable".
- **`toCanonical` throws on empty `homeDir`.** `FilePane.svelte`'s `userHomePath` is fetched async on mount and starts
  `''`. The pane-level `canonicalPath` `$derived` returns `null` while it's empty, so reactive callers must guard on
  `canonicalPath !== null` rather than catch the throw.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
