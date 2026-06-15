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
