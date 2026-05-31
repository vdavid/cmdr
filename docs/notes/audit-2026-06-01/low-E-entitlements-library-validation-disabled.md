# Hardened-runtime exceptions: unsigned-executable-memory + disabled library validation

**Severity:** low **Lens:** E — macOS platform **Confidence:** high

## Location

`apps/desktop/src-tauri/Entitlements.plist:11-15`

## What

The production code-signing entitlements grant two hardened-runtime exceptions:
`com.apple.security.cs.allow-unsigned-executable-memory` and `com.apple.security.cs.disable-library-validation`. The
first is genuinely required by WKWebView's JIT. The second (disable-library-validation) lets the process load libraries
NOT signed by Apple or by the same Team ID — it's there so Tauri/wry's WebView framework loads. With library validation
off, any dylib an attacker can plant where the process will `dlopen`/`DYLD`-load it (for example via a writable
`@rpath`/`DYLD_LIBRARY_PATH` entry, or the AI subsystem's `DYLD_LIBRARY_PATH` set in `ai/process.rs`) runs in-process
without a signature check.

## Why it matters

This is a standard Tauri requirement, not a Cmdr-specific mistake, and the app is sandboxed-out anyway (no App Sandbox;
it's a Developer ID file manager with broad FS access). The realistic attack needs a local write primitive into a load
path, at which point the attacker already has a foothold. Worth recording so it's a conscious accepted posture rather
than an unnoticed one, and so the AI subsystem's `DYLD_LIBRARY_PATH` usage is reviewed against it: the bundled
`llama-server` dylibs are loaded with library validation disabled, so their integrity rests entirely on bundle signing +
the path being inside the (root-owned, in `/Applications`) bundle.

## Evidence

```xml
<!-- WebView (WKWebView) needs unsigned executable memory for JIT -->
<key>com.apple.security.cs.allow-unsigned-executable-memory</key>
<true/>
<!-- Allow loading Tauri's WebView framework without Apple-signed validation -->
<key>com.apple.security.cs.disable-library-validation</key>
<true/>
```

## Suggested fix

Keep `allow-unsigned-executable-memory` (unavoidable for WKWebView JIT). For `disable-library-validation`, confirm
whether the current Tauri/wry version still needs it — recent Tauri builds on Developer ID often work without it if all
bundled dylibs (including the llama-server set) are signed with the same Team ID during the build. If they're already
signed by `Rymdskottkarra AB`, drop the entitlement and rely on Team-ID library validation. If it's still required,
leave a comment in the plist naming the exact framework/dylib that forces it, so a future cleanup doesn't have to
rediscover the reason.

## Notes

No hardened-runtime weakening beyond these two keys; no `com.apple.security.cs.allow-dyld-environment-variables` or
`disable-executable-page-protection`, which is good. The `ai/process.rs` `DYLD_LIBRARY_PATH` is set on the spawned
llama-server child, not the main app, but the disabled library validation applies to the whole signed binary's load
behavior — worth the cross-reference.
