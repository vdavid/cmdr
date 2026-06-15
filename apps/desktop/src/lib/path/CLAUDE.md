# Path module

Path helpers that are safe to call regardless of where a path came from (raw IPC return, persisted tab state, `~`-rooted
UI shortcut, virtual-volume URL).

## `canonical.ts` — `CanonicalPath` brand

A `CanonicalPath` is a string that has been verified safe for `dirname` / `basename` arithmetic: absolute POSIX paths
(`/Users/foo`) or virtual-volume URLs (`mtp://...`, `smb://...`, `search-results://...`). The brand excludes `~`-rooted
paths, relative paths, and anything else where naïve `lastIndexOf('/')` would silently produce the wrong answer.

Construct only via `toCanonical(raw, homeDir)`. Never cast. `parentOf` / `basenameOf` take `CanonicalPath` and are the
only blessed way to do path arithmetic in the frontend.

## Decision / why

**The brand is local, not viral.** Only the few path-arithmetic operations (`parentOf`, `basenameOf`) require the brand;
every other path-typed variable stays `string`. `CanonicalPath` is assignable to `string`, so branded values flow
outward freely; `string` is not assignable to `CanonicalPath`, so the conversion has to be explicit. No
`CanonicalPath | string` unions anywhere.

**Why we have this at all.** `FilePane.svelte`'s `currentPath` is sometimes the literal string `"~"` (it's the default
tab path and the home-shortcut target — the backend re-expands it on every IPC). A user pressing Enter on the `..` row
from `~` used to land at `/` because `"~".lastIndexOf('/')` is `-1` and the fallback was `/`. Backspace had the same
bug, and so did the `currentFolderName` derivation (`"~".split('/').pop() === "~"`). The brand makes those derivations
impossible without first canonicalizing.

## Gotchas

**`toCanonical` throws when `homeDir` is empty.** `userHomePath` in `FilePane.svelte` is fetched async on mount
(`homeDir()` from `@tauri-apps/api/path`) and starts as `''`. Reactive call sites that derive parent on first render
must guard on `canonicalPath !== null` until `userHomePath` resolves — the pane-level `canonicalPath` `$derived` returns
`null` while `userHomePath` is empty so callers don't need to catch the throw.

**Virtual-volume URLs use slash arithmetic too.** `mtp://device/storage/Music` → `mtp://device/storage` via simple
`lastIndexOf('/')`. The brand exists to make this _safe to assume_: anything that survived `toCanonical` is known to
have a slash where the parent boundary lives.

Full details: [DETAILS.md](DETAILS.md).
