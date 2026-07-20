# Detecting third-party File Provider domain roots

Research note. Question: can Cmdr (unsandboxed, Full Disk Access, not a File Provider extension host) detect generically
that a directory is the root of someone else's File Provider domain, so the indexer can treat it as a foreign volume
instead of an ordinary folder?

**Answer: yes, two independent detectors, and they agree.** Everything below was measured on this machine (macOS 26.5.2,
build 25F84, Apple Swift 6.3.3), 2026-07-20. Probe sources: `scratchpad/fpprobe{,2,3}.swift` (session scratchpad, not in
the tree).

Recap of what's already established: File Provider domains are not mount points. Every domain root reports the same
`st_dev` as `$HOME` (16777231, `/dev/disk3s5`) and none appear in `mount`. So `st_dev` transitions and `statfs` fstype
are both useless here.

## Detector 1: the `com.apple.file-provider-domain-id` xattr (fast path)

Every domain root carries an extended attribute `com.apple.file-provider-domain-id` whose value is
`<provider extension bundle id>/<domain identifier>`. Children inside the domain do not carry it.

Measured values:

- `~/Library/CloudStorage/Dropbox` → `com.getdropbox.dropbox.fileprovider/c840514d-cf5c-4456-a70c-df4e791dfaf9`
- `~/Library/CloudStorage/GoogleDrive-<account>` → `com.google.drivefs.fpext/gdrive-103986087393247194397`
- `~/Library/CloudStorage/MacDroid-googlePixel9ProXL` →
  `us.electronic.mas.macdroid.mountprovider/us.electronic.mas.macdroid.mountprovider.19ef05361949…`
- `~/Library/Mobile Documents` → `com.apple.CloudDocs.iCloudDriveFileProvider/4B4D7739-C48A-43D1-A2AA-55926959D3DF`

Note the last one: iCloud Drive's domain root is `~/Library/Mobile Documents` itself, not
`~/Library/Mobile Documents/com~apple~CloudDocs` (that path is a child item inside the domain). So this detector also
catches domains that live outside `~/Library/CloudStorage`, which the "children of `~/Library/CloudStorage`" heuristic
misses.

**Cost: about 5 µs per `getxattr` call** (2,000-call loop per path, `XATTR_NOFOLLOW`), essentially the same on a hit and
a miss, and the same for the offline domain as for the live ones. It's a plain APFS xattr read; no XPC, no provider
process involved, so there's no hang risk.

**False positives:** a full-home sweep (93,868 directories, depth 6) and a `/` sweep (25,300 directories, depth 4) found
the xattr on exactly the four user-visible roots above plus the four private backing directories under
`~/Library/Application Support/FileProvider/<UUID>/` (one per domain, carrying the same value). Nothing else. If the
indexer walks Application Support, those four need excluding, or use detector 2 to reject them (it does, see below).

**Caveat:** this xattr is not in Apple's public documentation. It's an implementation detail of `fileproviderd`, stable
across all four providers here but not contractual.

## Detector 2: `NSFileProviderManager.getIdentifierForUserVisibleFile(at:)` (authoritative)

`+[NSFileProviderManager getIdentifierForUserVisibleFileAtURL:completionHandler:]`, macOS 11.0+ (SDK header
`FileProvider.framework/Headers/NSFileProviderManager.h`, `FILEPROVIDER_API_AVAILABILITY_V3_IOS` =
`API_AVAILABLE(macos(11.0), ios(16.0))`).

**It works for other vendors' domains**, from a plain non-extension process. It returns the domain identifier plus an
item identifier, and the item identifier is `NSFileProviderRootContainerItemIdentifier` exactly at the domain root and
an opaque provider id for anything inside. For a path in no domain (including a path that doesn't exist) it returns
`NSCocoaErrorDomain` code 4 (`NSFileNoSuchFileError`), so it can't distinguish "plain folder" from "missing"; that's
fine for this use.

**Header wording vs. observed behavior:** the header says "Calling this method on a file which doesn't reside in your
provider/domain … will return the Cocoa error `NSFileNoSuchFileError`", which reads as caller's-own-domain-only.
Observed behavior is broader: it resolves all four foreign domains. Treat the cross-vendor capability as undocumented
behavior that Apple could tighten, and keep detector 1 (or the CloudStorage-location fallback) as a backstop.

**Cost:** 500 sequential calls per path from a non-main thread, each blocking on a semaphore:

- online domain root (Dropbox): mean 0.342 ms, p50 0.317, p99 0.928, max 3.330
- offline domain root (MacDroid): mean 0.286 ms, p50 0.281, p99 0.384, max 0.540
- plain folder (miss): mean 0.446 ms, p50 0.442, p99 0.604, max 0.716

That's an XPC round trip to `fileproviderd`. At ~0.35 ms it is fine per candidate boundary and impossible per directory
(600,000 × 0.35 ms ≈ 210 s).

**Entitlements: none.** The probe is an ad-hoc linker-signed CLI binary with no entitlements, no Info.plist, and no app
bundle, running unsandboxed. Not determined: whether it also works without Full Disk Access (the probe inherited the
terminal's TCC grants).

**Offline providers are fine.** The MacDroid domain's Android device was not attached and its provider extension
(`…mountprovider`) was not running during every measurement above; only MacDroid's Finder Sync extension was alive. Both
detectors resolved the domain anyway, and the API calls did not launch the provider extension: `fileproviderd` answers
from its own database, no vendor code runs. The offline domain was in fact the _fastest_ of the three.

**Hang risk:** no timeout in 1,500+ calls, and the completion handler fires on an internal queue (no run loop needed),
so the semaphore-blocking pattern works off the main thread. That is not a proof it can never hang: it is an XPC call.
If it's used from a walk thread, wrap it in a timeout and treat expiry as "not a domain".

## Dead end: `getDomainsWithCompletionHandler`

Confirmed useless for us, as expected. From a non-extension-hosting app it returns zero domains and
`NSFileProviderErrorDomain` code -2001 (`NSFileProviderErrorProviderNotFound`) wrapping -2014
(`NSFileProviderErrorApplicationExtensionNotFound`). It only ever enumerates the calling app's own extension's domains.

## Also dead ends

- `st_flags` is 0 on every domain root and on plain folders (`stat -f '%Xf'`).
- No distinguishing mode or ownership: the roots are plain `drwx------` / `dr-x------` user-owned directories.
- Other xattrs are vendor-specific (`com.dropbox.*`, `com.apple.FinderInfo`) and absent on some roots, so they can't
  serve as a generic marker.

## Recommendation for the indexer

Use the xattr as the cheap in-walk detector and the API to confirm a hit:

1. During the walk, `getxattr(path, "com.apple.file-provider-domain-id", …, XATTR_NOFOLLOW)` on each directory. ~5 µs,
   no hang risk. Absent (the overwhelming majority) → ordinary folder, done.
2. On a hit, call `getIdentifierForUserVisibleFile` once with a timeout. Item identifier ==
   `NSFileProviderRootContainerItemIdentifier` → this is a user-visible domain root, treat as a foreign volume. An error
   → it's a private backing directory under `~/Library/Application Support/FileProvider/`, not a user-visible root.

Step 1 alone over 600,000 directories is ~3 s of syscall time, so scope it if that matters (only under `$HOME`, or only
where a `CLAUDE.md`-documented candidate boundary is plausible); step 2 fires a handful of times per machine.

The location heuristic ("every child of `~/Library/CloudStorage` is a domain root") is a valid last-resort fallback and
is where Apple puts third-party domains, but it is strictly worse: it misses `~/Library/Mobile Documents`, and it would
break for any provider that registers a domain elsewhere.

### Calling it from Rust

`objc2-file-provider` 0.3.2 (published 2025-10-04, same objc2 0.6 / 0.3.2 family as the bindings already in
`Cargo.toml`) exposes
`NSFileProviderManager::getIdentifierForUserVisibleFileAtURL_completionHandler(&NSURL, &DynBlock<dyn Fn(*mut NSFileProviderItemIdentifier, *mut NSFileProviderDomainIdentifier, *mut NSError)>)`
behind the `block2`, `NSFileProviderDomain`, and `NSFileProviderItem` features. Bridge the completion handler to a
`std::sync::mpsc` channel and `recv_timeout`. The xattr step needs no crate beyond `libc::getxattr`.
