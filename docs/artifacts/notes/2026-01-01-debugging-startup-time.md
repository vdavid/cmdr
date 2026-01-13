# Debugging slow app startup (2026-01-01)

## Problem description

- The app takes **~20 seconds** from launch to becoming usable
- User reports: "5 seconds for the loading indicator to appear, then 15 more seconds to load"
- Yesterday it was 3-5 seconds total
- Issue started suddenly - user verified even 5-day-old git commits are now slow
- Other file managers on the system start fast, so it's not a general system issue

## Key findings

### Timing breakdown (from benchmarking)

| Phase                  | Duration    | Notes                                         |
| ---------------------- | ----------- | --------------------------------------------- |
| Rust setup             | ~30ms       | ✅ Fast - all init functions complete quickly |
| HTML body parsing      | ~5.4s       | Body inline script runs at this point         |
| CSS/JS Bundle loading  | **~14-15s** | ⚠️ **THE PROBLEM**                            |
| Svelte init + settings | ~10ms       | ✅ Fast                                       |
| Directory loading      | ~10ms       | ✅ Fast                                       |

### Key observation

The `+page.svelte` script doesn't start executing until **~20 seconds** after page navigation:

```
[vite] connected at 00:02:55.951Z
[STARTUP] +page.svelte script start at 20778ms  (00:03:17)
```

This is a **21-second gap** between vite connecting and the script executing!

The delay is happening **inside the WKWebView** - not in Rust, not in network I/O.

## Things we've tried

### 1. ❌ Checked Rust backend timing

- Added `[SETUP]` timing logs to `lib.rs` setup function
- Result: Rust setup completes in ~30ms
- **Not the cause**

### 2. ❌ Checked font metrics loading

- Font metrics are loaded from disk cache (`system-400-12.bin`, 428KB)
- Log shows: `[FONT_METRICS] Loaded metrics from disk for font: system-400-12`
- Completes in ~25ms
- **Not the cause**

### 3. ❌ Checked path resolution / IPC calls

- Added timing to `loadAppStatus`, `resolvePathWithFallback`
- Each `pathExists` IPC call: ~1ms
- Total path resolution: ~10ms
- **Not the cause**

### 4. ❌ Cleared WebKit cache

- Deleted `~/Library/Caches/cmdr/WebKit/` (58MB)
- Restarted app
- **No improvement** - still 20+ seconds

### 5. ❌ Verified Dropbox is not the issue

- User quit Dropbox completely
- Folder is now just a regular folder
- **No improvement** - still slow

### 6. ❌ Tried production build

- User ran `pnpm tauri dev --release`
- Loading indicator appeared earlier but total load still ~19 seconds
- **Marginally helped but not the fix**

### 7. ❌ Git bisect on recent commits

- User manually tested 5-day-old commits
- Even old commits are now slow!
- **Points to system-level change, not code change**

### 8. ❌ Disabled MCP Bridge plugin

- Commented out `tauri_plugin_mcp_bridge::init()` in `lib.rs`
- Rebuilt and ran
- **No improvement** - still slow

### 9. ❌ Verified network is fast

- `curl` fetches modules from vite dev server in <1ms
- **Not a network issue**

## Current hypothesis

Something on the macOS system is causing WKWebView to be extremely slow at:

- Loading JavaScript modules
- Or executing JavaScript

Possible causes to investigate:

1. macOS system update affecting WKWebView
2. Security software intercepting webview traffic
3. WebKit config/state corruption
4. macOS power management throttling
5. Spotlight/mdworker indexing affecting file access

## Files modified for debugging

1. `src/routes/+page.svelte` - Added timing logs
2. `src/lib/file-explorer/DualPaneExplorer.svelte` - Added onMount timing
3. `src/lib/app-status-store.ts` - Added timing throughout
4. `src/app.html` - Added inline script timing
5. `src-tauri/src/lib.rs` - Added setup timing + temporarily disabled MCP

## Next steps to try

1. Check macOS Console for WKWebView errors
2. Try a fresh Tauri project to isolate the issue
3. Check Activity Monitor for unusual CPU/disk during startup
4. Verify macOS system settings (battery saver, app nap, etc.)
5. Try running outside of Dropbox folder entirely (copy to /tmp)
6. Check for any browser extensions or system-level JavaScript blockers

## System log analysis (2026-01-01 ~01:40)

### Key findings from `log show`

1. **PlugInKit errors** - Multiple `Connection interrupted` errors:

    ```
    [com.apple.PlugInKit:xpc] XPC error talking to pkd: Connection interrupted
    [com.apple.extensionkit:NSExtension] errors encountered while discovering extensions
    ```

2. **Precondition failure in renderbox**:

    ```
    [com.apple.renderbox:error] precondition failure: <private>
    ```

3. These errors happen right at startup, possibly delaying WebView initialization.

### Possible causes

- macOS `pkd` (PlugInKit daemon) is misbehaving
- System extensions taking long to enumerate
- Could be related to a macOS update or state corruption

### Potential fixes to try

1. Restart the `pkd` daemon: `sudo killall -9 pkd`
2. Clear plugin caches
3. Restart macOS
4. Check if disabling extensions helps

## ROOT CAUSE FOUND: Tailwind CSS v4 (2026-01-01 01:45)

### Test results

| Configuration        | Script start time |
| -------------------- | ----------------- |
| With Tailwind v4     | ~16-20 seconds    |
| **Without Tailwind** | **~5 seconds** ✅ |

### Analysis

Disabling `@import 'tailwindcss';` in `src/app.css` reduced startup from 16-20s to ~5s!

Tailwind CSS v4 uses a JIT (Just-In-Time) compiler that scans source files at runtime in dev mode. This scanning process
appears to be extremely slow, possibly due to:

1. Tailwind v4's new CSS-first configuration scanning all project files
2. Interaction with WKWebView's CSS processing
3. The PlugInKit errors potentially affecting file scanning performance

### Possible solutions

1. **Add explicit source constraints** - Use `@source` directive to limit scanning
2. **Pre-build Tailwind CSS** - Generate CSS at build time instead of JIT
3. **Downgrade to Tailwind v3** - May have faster dev mode
4. **Use production build for dev** - `pnpm tauri dev --release` with pre-built CSS
5. **Remove Tailwind entirely** - Use vanilla CSS with CSS custom properties (already have design tokens)

## RESOLUTION (2026-01-01 01:48)

**Solution implemented:** Removed Tailwind CSS v4 completely.

The codebase was not using ANY Tailwind utility classes - only semantic CSS classes with CSS custom properties (design
tokens). Tailwind was installed but doing nothing except slowing down startup with its JIT scanning.

**Changes made:**

1. `src/app.css` - Removed `@import 'tailwindcss'`
2. `vite.config.js` - Removed `@tailwindcss/vite` plugin

**Final result:**

- Before: ~20 seconds to script start
- After: **~4.7 seconds** to script start ✅
- Improvement: ~75% faster

**Why this happened recently:** The PlugInKit errors in macOS system logs suggest something changed at the system level
that made file scanning slower. Combined with Tailwind v4's aggressive file scanning, this caused the massive slowdown.
Even old code was slow because the issue is system-level combined with Tailwind's scanning behavior, not code changes.

## Future optimization ideas

The remaining ~5 second delay before app.html renders is WKWebView initialization, which is harder to optimize.
Potential areas to investigate:

1. **window-state plugin** - May be restoring window position slowly at startup
2. **WKWebView preloading** - Tauri may support warming up the webview earlier
3. **Reduce plugins** - Each Tauri plugin adds overhead during initialization
4. **Production builds** - Use `--release` for faster native code execution
5. **macOS system state** - The PlugInKit errors suggest a system-level issue that could be resolved by restarting macOS
   or clearing system caches

## Debug code cleanup

All temporary debug logging has been removed from:

- `src/routes/+page.svelte`
- `src/lib/file-explorer/DualPaneExplorer.svelte`
- `src/lib/app-status-store.ts`
- `src/app.html`
- `src-tauri/src/lib.rs`
