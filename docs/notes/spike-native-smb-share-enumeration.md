# Spike: native macOS SMB share enumeration (replacing the `smbutil view` shell-out)

**Date:** 2026-05-31 · **Platform probed:** macOS 26.5 (build 25F71), SDK MacOSX26.5

## The question

Cmdr's SMB share listing falls back, for older servers where the pure-Rust `smb2` crate's RPC fails, to shelling out to
`smbutil view //user:password@host`. That leaks the password into the process argument list (`ps aux` /
`/proc`-equivalent) for the ~sub-second the child runs. (See the `// SECURITY:` block in
`apps/desktop/src-tauri/src/network/smb_smbutil.rs` and the "Credential channel" section of
`apps/desktop/src-tauri/src/network/CLAUDE.md`.)

Is there a native macOS framework API to **enumerate the shares** on an SMB server, passing credentials **in memory**
(never via argv/CLI), callable from Rust FFI, to replace that shell-out?

## Verdict: **YES-BUT-PRIVATE-SPI, and only for the auth half — enumeration isn't a framework API at all**

Two findings, and the second is the one that decides it:

1. **Authenticating an SMB session with in-memory credentials is doable** via `SMBClient.framework`'s `SMBOpenServerEx`.
   It's **private SPI** (symbols ship, no public headers). A probe authenticated correctly against the Docker test
   containers with creds passed in memory (no child process, no argv). This half cleanly closes the leak.

2. **Enumerating the shares is NOT a framework API.** The framework gives you the authenticated session and the IPC$
   tree-connect, but the actual "list the shares" logic — `NetShareEnum` (srvsvc DCE/RPC) **and** `RapNetShareEnum` (the
   legacy RAP path that is the entire reason Cmdr falls back to `smbutil` for old servers) — lives **inside the
   `smbutil` binary itself**, not in any framework Cmdr could link. There is no `SMBClient` export that returns the
   share list as a dictionary.

So you can't just "call the framework instead of the CLI." You'd link a private framework for the auth, then
**reimplement the srvsvc + RAP enumeration yourself**. And if we're reimplementing enumeration anyway, the right home
for it is Cmdr's own `smb2` crate (which already does srvsvc and authenticates in memory), not a fragile private-SPI
FFI. **This is not the clean drop-in replacement it looked like.**

## Evidence

### `smbutil view` calls into `SMBClient.framework` — but does the enumeration itself

`smbutil` (`/usr/bin/smbutil`, universal arm64e + x86_64) links the **private** `SMBClient.framework` and
`DCERPC.framework` (`otool -L`). Disassembling its `cmd_view` (`otool -tvV`) shows the exact call chain:

```
_cmd_view:
    ... getopt, build URL string ...
    bl  _SMBOpenServerEx            ; framework export — opens authenticated session
    bl  _ntstatus_to_err
    ... print header ...
    bl  _smb_netshareenum          ; smbutil-INTERNAL (not a framework symbol)
    bl  _createShareArrayFromShareDictionary

_smb_netshareenum:                 ; lives in the smbutil binary
    bl  _SMBNetFsTreeConnectForEnumerateShares   ; framework export — connect IPC$
    bl  _NetShareEnum              ; smbutil-INTERNAL — srvsvc DCE/RPC enumeration
    bl  _RapNetShareEnum           ; smbutil-INTERNAL — legacy RAP fallback for old servers
```

`SMBOpenServerEx` call shape (from the disassembly):
`SMBOpenServerEx(x0 = "//user:pass@host[:port]" C string, x1 = &handle, x2 = options)`, returns NTSTATUS. The
credential-bearing string is an **in-memory C string** — there is no child process, so nothing reaches any argv. That's
the channel that closes the `ps`-visible leak.

`NetShareEnum` and `RapNetShareEnum` are **defined inside `/usr/bin/smbutil`** (they appear as local symbols with bodies
in the same `otool -tvV` dump, calling `_smb_rap_parserqparam`, `_smb_rap_rqparam_z`, etc.). They are **not** exported
by `SMBClient.framework`. `smb_netshareenum` is likewise smbutil-local. Confirmed by dumping all framework exports
(`dyld_info -exports`): the only share-adjacent exports are `SMBGetShareAttributes`,
`SMBNetFsTreeConnectForEnumerateShares`, `SMBMountShare*`, `SMBCheckForAlreadyMountedShare` — none returns the share
list. `smb_netshareenum` / `NetShareEnum` / `EnumerateShares` are absent from the export table.

### Public vs private/SPI — it's PRIVATE SPI

- **`SMBClient.framework` is a PrivateFramework with NO public headers.** In the SDK
  (`MacOSX26.5.sdk/.../PrivateFrameworks/SMBClient.framework/`) there is only a `SMBClient.tbd` (linker stub) — no
  `Headers/` directory. Grepping the **entire SDK** for `SMBOpenServerEx`, `SMBNetFsTreeConnectForEnumerateShares`,
  `SMBNetFsCreateSessionRef` returns **zero** header hits. So the API is private/SPI: linkable (the `.tbd` exports the
  symbols, listed below) but undeclared and unsupported.

  ```
  SMBClient.tbd exports (relevant): _SMBOpenServerEx, _SMBNetFsCreateSessionRef, _SMBNetFsOpenSession,
    _SMBNetFsGetServerInfo, _SMBNetFsTreeConnectForEnumerateShares, _SMBNetFsCloseSession
  ```

- **`NetFS.framework` is public, but exposes only mounting, not enumeration.** `NetFS.h` publicly declares
  `NetFSMountURLSync` / `NetFSMountURLAsync` / `NetFSMountURLProbe` (Cmdr already uses the sync one in
  `apps/desktop/src-tauri/src/network/mount.rs`). The header _mentions_ "EnumerateShares methods" and `GetServerInfo`,
  and defines the in-memory credential keys `kNetFSUserNameKey` (`"UserName"`) and `kNetFSPasswordKey` (`"Password"`) —
  but the only place an `EnumerateShares` function is **declared** is `NetFSPlugin.h`, inside the
  `NetFSMountInterface_V1` CFPlugInCOM vtable. **That vtable is a plugin-author SPI: it's the interface a filesystem
  plugin _implements_ for NetFS to call, not a client API you call to enumerate.** `SMBClient.framework`'s `_SMBNetFs*`
  exports are precisely Apple's SMB implementation of that vtable. There is no public client-side
  `NetFSEnumerateShares(...)`.

So: the **public** NetFS surface can mount but can't enumerate; the **private** SMBClient surface can authenticate (in
memory) but still doesn't hand you the share list — that part is smbutil's own code.

### Credential channel — confirmed in-memory (the leak does close, for the auth step)

- `SMBOpenServerEx` takes creds embedded in an in-memory C string. No subprocess ⇒ no argv ⇒ no `ps` exposure.
- The NetFS route (`SMBNetFsOpenSession` / `SMBNetFsGetServerInfo`) takes creds as a `CFDictionary` with
  `kNetFSUserNameKey` / `kNetFSPasswordKey` — also pure in-memory. This is the same shape Cmdr already uses for guest
  mounts (`kNetFSUseGuestKey`) in `mount.rs`.

Either way, credentials never transit argv or a file path. The leak is genuinely closed for the authenticate step.

### Probe — it ran, against the Docker test containers

`/tmp/smbprobe.c` (throwaway): declares `SMBOpenServerEx` / `SMBReleaseServer` ourselves (no public header), links
`-F/System/Library/PrivateFrameworks -framework SMBClient -framework CoreFoundation`, opens a session with
`kSMBOptionNoPrompt | kSMBOptionSessionOnly`, prints the NTSTATUS. Built clean with `clang`. Results against the running
`smb-consumer-*` containers:

| Target                            | Creds                    | NTSTATUS     | Meaning                           |
| --------------------------------- | ------------------------ | ------------ | --------------------------------- |
| auth container `localhost:10481`  | `testuser` / `testpass`  | `0x00000000` | `STATUS_SUCCESS` — session opened |
| auth container `localhost:10481`  | `testuser` / `wrongpass` | `0xc000006d` | `STATUS_LOGON_FAILURE`            |
| guest container `localhost:10480` | none                     | `0x00000000` | `STATUS_SUCCESS`                  |

Two things this proves beyond the auth itself: credentials pass **in memory** (no child process — the probe links no
`posix_spawn`/`exec`), and the API returns a **typed NTSTATUS** (no error-string matching needed — Cmdr's
`no-error-string-match` rule would be satisfiable, e.g. branch on `0xc000006d` for auth-failed).

It does **not** prove enumeration end-to-end, because (per the disassembly) the framework doesn't enumerate — that's
smbutil-internal code. Extending the probe to list shares would mean reimplementing srvsvc/RAP, which is the whole point
of the verdict.

### Codesigning / hardened runtime — no blocker

Re-signed the probe with `codesign --force --options runtime` (hardened runtime, the notarization requirement) and it
still authenticated. Private-framework linkage is **not** gated by entitlements or hardened runtime; only TCC-protected
resources are. Cmdr is directly distributed (not App Store), so private-framework use is permitted by notarization. The
risk is fragility across macOS versions, not signing.

## How Cmdr would call it (FFI sketch) — if we went this way

Cmdr already does this exact pattern in `mount.rs`: `#[link(name = "NetFS", kind = "framework")]` + `unsafe extern "C"`
for `NetFSMountURLSync`, building `CFString`/`CFURL`/`CFDictionary` via the `core-foundation` crate. The SMBClient
equivalent:

```rust
#[link(name = "SMBClient", kind = "framework")]
unsafe extern "C" {
    fn SMBOpenServerEx(target: *const c_char, out_handle: *mut *mut c_void, options: u64) -> i32; // NTSTATUS
    fn SMBNetFsTreeConnectForEnumerateShares(handle: *mut c_void /* ... */) -> i32;
    fn SMBReleaseServer(handle: *mut c_void) -> i32;
}
```

`#[link(name = "SMBClient", kind = "framework")]` needs a `-F/System/Library/PrivateFrameworks` search path (a
`build.rs` `cargo:rustc-link-search=framework=...`), since private frameworks aren't on the default link path. **No new
entitlement.** But the FFI is the easy part — it only gets you the authenticated session. You'd still have to hand-roll
the srvsvc `NetShareEnum` RPC and the RAP fallback on top, with no headers and no support contract.

## Risks

- **Private SPI fragility.** No headers, no API contract. Apple can rename/remove `SMBOpenServerEx` or change its
  signature in any macOS update; the app would break at runtime with a dyld symbol-not-found (or worse, a silent ABI
  mismatch). The mitigation (weak-link + graceful fallback to… smbutil) reintroduces the thing we're trying to remove.
- **You still own the enumeration.** The hard, server-compatibility-sensitive part (srvsvc vs RAP for ancient Samba) is
  not in the framework. Reimplementing it correctly is exactly what `smb2` already does for modern servers and what
  smbutil's RAP path does for old ones.
- **Codesigning:** none (verified above).

## Rough effort estimate

- Private-SPI auth-only FFI + reimplement srvsvc/RAP enumeration in Rust to fully replace smbutil: **multi-day**, plus
  ongoing version-fragility maintenance. Net: high cost, fragile, and it duplicates `smb2`'s domain.
- Alternative — extend `smb2` (Cmdr's own crate) to handle the old-server case that currently forces the smbutil
  fallback (e.g. add/strengthen the RAP `NetShareEnum` fallback, which is the actual gap): **comparable or less**
  effort, no private SPI, no FFI, fully supported, cross-platform, and it shrinks the smbutil dependency to zero instead
  of swapping one private dependency for another.

## Bottom line / recommendation

**Not worth it as framed.** The native API only solves the easy half (in-memory auth); the half that matters
(enumerating shares on the exact old servers that trigger the fallback) isn't a framework API — it's smbutil's own
srvsvc/RAP code. Linking the private `SMBClient.framework` would buy us an in-memory-auth handle and then leave us
reimplementing enumeration anyway, on a fragile, unsupported, version-sensitive SPI.

If the goal is to kill the `smbutil view` argv leak, the cleaner path is to **close the gap in `smb2`** (Cmdr's own
crate) so the smbutil fallback is no longer needed for those old servers — same enumeration work, but in supported,
cross-platform, in-memory Rust we control, rather than behind private Apple SPI. Short of that, the residual leak is
already tightly scoped (rare old-server fallback only; primary mount via `NetFSMountURLSync` and primary enumeration via
`smb2` never expose the password) and documented, so leaving it as-is is defensible until `smb2`'s fallback lands.
