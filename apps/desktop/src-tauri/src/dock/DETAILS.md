# Dock: details

## The `persistent-apps` entry shape

Every tile in a live Dock looks like this (verified on macOS 26.0, `defaults read com.apple.dock
persistent-apps` on a real machine, 2026-09-09):

```
GUID: 3733101419                      ← ten digits, unique per tile
tile-type: "file-tile"
tile-data:
    book: <568-byte opaque bookmark blob>
    bundle-identifier: "com.veszelovszki.cmdr"
    dock-extra: 0
    file-data: { _CFURLString: "file:///Applications/Cmdr.app/", _CFURLStringType: 15 }
    file-label: "Cmdr"
    file-mod-date: 238370260588490
    file-type: 41
    is-beta: 0
    parent-mod-date: 117844888796596
```

`_CFURLStringType` 15 means "absolute URL string", so the path is percent-encoded
(`file:///Applications/Google%20Chrome.app/`) and carries a trailing slash. Finder's tile is not in
this array: the Dock draws it itself, which is what makes index 0 the leftmost slot anyone can
reach.

### The entry we write, and why it's smaller

`entries::app_tile` writes six keys and no bookmark:

```
GUID, tile-type: "file-tile"
tile-data: { bundle-identifier, file-data: { _CFURLString: "/Applications/Cmdr.app",
             _CFURLStringType: 0 }, file-label: "Cmdr", file-type: 41 }
```

**`_CFURLStringType` 0** means `_CFURLString` is a plain absolute POSIX path rather than a URL. It's
the form with no percent-encoding in it, so nothing in this module can mis-escape a path with a
space, a `%`, or a non-ASCII character in it. The Dock normalizes the entry to the type-15 URL form
on its next write. `entries::entry_path` reads both forms, because our own freshly-written tile is
type 0 and every neighbour is type 15.

**No `book` blob.** The `book` value is an opaque `CFURLCreateBookmarkData` bookmark that lets the
Dock follow the app if it moves. **Not verified first-hand that an entry without one survives** —
verifying it means writing to the real `com.apple.dock`, which this milestone was explicitly barred
from doing. The evidence it rests on: `dockutil` (github.com/kcrawford/dockutil, 1,598 stars, last
pushed 2025-12-30, the tool every MDM deployment uses for this) writes exactly the six keys above,
with `_CFURLStringType: 0`, no `book`, and no `bundle-identifier`, and has done so across every
macOS release since Big Sur. **Manual confirmation is still owed**; see the manual check below. If it
turns out a bookmark is required, `CFURLCreateBookmarkData` is the API to reach for — ❌ never
fabricate the blob.

**`bundle-identifier` is ours on top of dockutil's minimum**, taken from `NSBundle::mainBundle()` so
the tile always names the app that wrote it. It's a key macOS writes on every entry itself, and
having it means `holds_app` finds our own tile by its strongest key before the Dock has rewritten
anything.

The keys deliberately left out (`book`, `dock-extra`, `is-beta`, `file-mod-date`,
`parent-mod-date`) are all things the Dock computes; writing a guess for any of them would be worse
than writing nothing.

## Why the identifier decides "already pinned"

`entries::holds_app` checks the bundle identifier first and falls back to the decoded path. The
identifier is an exact string with no encoding to get wrong. `_CFURLString` has three ways to say
"the same app" and not compare equal: percent-encoding, the trailing slash, and the file-reference
form (`file:///.file/id=6815814.9265358/`) macOS substitutes after some moves. A tile in that last
form names Cmdr perfectly and yields a path matching nothing on disk.

Either key matching answers yes, which biases toward "already there". The two failure modes are not
symmetric: a false yes costs a hint nobody sees, a false no puts a second Cmdr tile in the Dock.

## The CFPreferences boundary

`prefs.rs` addresses the domain fully-qualified — `CFPreferencesCopyValue` / `CFPreferencesSetValue`
with `kCFPreferencesCurrentUser` + `kCFPreferencesAnyHost` — rather than the `…AppValue…`
shorthand. `CFPreferencesCopyAppValue` folds in the managed and any-user layers, which is the right
answer to "what does the Dock see" and the wrong one to "what am I about to overwrite".
`CFPreferencesAppValueIsForced` asks the managed question on its own, and a managed Dock is refused
as `ManagedDock` rather than written to and silently ignored.

Values cross the boundary as **binary plist bytes**: `CFPropertyListCreateData` on the way in,
`CFPropertyListCreateWithData` on the way out, with the `plist` crate on the Rust side. That keeps
the whole CF surface to four calls and leaves every decision above it operating on `plist::Value`,
testable with no preferences domain anywhere near it. The alternative — walking `CFArray` and
`CFDictionary` by hand — would have spread `unsafe` across the module for no gain.

`CFPreferencesAppSynchronize` is what makes the value visible to another process, so a failed
synchronize is a failed write however well the set went (`PrefsError::NotFlushed`).

`cfprefsd` does not need restarting: it is the thing we wrote through, and the Dock reads back
through it. (`dockutil` used to restart it, then dropped that in favour of going through the
preferences API, which is the branch we're on.)

## Restarting the Dock

`NSRunningApplication::runningApplicationsWithBundleIdentifier("com.apple.dock")` then `terminate()`.
`launchd` brings the Dock straight back, re-reading its preferences. This is `dockutil`'s first
choice too, with `killall Dock` only as its fallback; the AppKit route needs no subprocess. Verified
first-hand only as far as the lookup: `restart::tests` asserts we find the running Dock and that it
is the Dock, and ❌ never terminates it.

`NSRunningApplication` is thread-safe and needs no `MainThreadMarker`, so the AppKit main-thread rule
in `../../CLAUDE.md` doesn't apply and the pin can run on the blocking pool like any other command.
CFPreferences is likewise thread-safe, and both are listed among the exemptions in
`../../DETAILS.md` § Which Apple APIs skip the main-thread rule.

## What we learned about TCC, and what we didn't

Established, on this machine (macOS 26.0, `cargo test` binary, unsigned dev build, 2026-09-09):

- **Reading `com.apple.dock` through CFPreferences raises no prompt and returns the real array.**
  `prefs::tests::the_real_dock_domain_is_readable_without_a_permission_prompt` does exactly this and
  passes.
- **Writing another application's preferences domain through CFPreferences works with no prompt.**
  `prefs::tests::an_array_survives_the_round_trip_through_cfprefsd` writes an array of Dock tiles to
  `com.getcmdr.docktest.roundtrip` and reads it back byte-identical.
- **`CFPreferencesAppValueIsForced("persistent-apps", "com.apple.dock")` is false here**, so no
  configuration profile manages this Dock.

Still unknown, and only a manual run can settle it:

- Whether writing `com.apple.dock` specifically raises a prompt. A scratch domain nobody owns is not
  proof about a domain Apple ships, and macOS has singled out individual domains before.
- Whether a **notarized, signed** build behaves the same as this unsigned test binary. Signing
  changes which TCC identity the request carries.
- Whether an entry with no `book` survives the Dock's parse (see above).

## The manual check

Cmdr is already in David's Dock, so the offer would currently answer `AlreadyPinned`. To test the
real write end to end:

1. Drag Cmdr out of the Dock (or right-click its tile → Options → uncheck "Keep in Dock").
2. Confirm it's gone from the stored array:
   ```
   defaults read com.apple.dock persistent-apps | grep -c veszelovszki
   ```
   Expect `0`.
3. Launch the installed `/Applications/Cmdr.app` and accept the nudge. (Until the toast lands, the
   equivalent is calling the `add_cmdr_to_dock` IPC command — `pnpm dev` won't do: a dev build has
   no `.app` ancestor and answers `NotABundle` by design.)
4. Confirm the tile is there, leftmost after Finder, and that the stored entry took:
   ```
   defaults read com.apple.dock persistent-apps | head -20
   ```
   The first entry should be Cmdr. If the Dock filled in a `book` blob and rewrote
   `_CFURLStringType` to 15, the minimum-entry assumption above is confirmed.

If step 4 shows a broken or question-mark tile, the bookmark is required after all: say so, and
reach for `CFURLCreateBookmarkData`.
