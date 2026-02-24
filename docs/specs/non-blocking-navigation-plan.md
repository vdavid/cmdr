# Non-blocking navigation: always-responsive UI

## Problem

Switching away from a slow or unresponsive network drive freezes the entire app. The user clicks "Macintosh HD" while on
a network share (for example, naspi over SMB), and gets the macOS spinning cursor with no spinner, no feedback, and no
way to cancel. The freeze can last 30–60+ seconds.

The streaming directory listing architecture is well-designed and already non-blocking — the issue is in the
**path-validation layer** that runs *before* any listing starts.

## Root cause

Every volume switch goes through `handleVolumeChange()` → `await determineNavigationPath()`, which makes **sequential
`pathExists()` IPC calls** to decide the best starting path. Each `pathExists` call hits `std::fs::metadata()` on the
Rust side, which is a kernel syscall. On a hung/slow network mount, macOS blocks that syscall for the full NFS/SMB
timeout (30–60 seconds), and the calls are sequential — worst case: 2–3 timeouts chained.

Although JS `await` yields to the event loop, no loading state is set before the await, so the user sees stale content
and the macOS spinning cursor (because the app isn't visually responding to the interaction).

### All affected call sites

| Location | Function | Why it blocks |
|----------|----------|---------------|
| `DualPaneExplorer.svelte:352` | `handleVolumeChange` | `await determineNavigationPath()` — 2–3 sequential `pathExists` calls |
| `DualPaneExplorer.svelte:394` | `handleCancelLoading` | `resolveValidPath()` walks parent tree — N sequential `pathExists` calls |
| `DualPaneExplorer.svelte:968` | `handleNavigationAction` | `await resolveValidPath()` on back/forward |
| `FilePane.svelte:937–943` | listing error handler | `pathExists` + `resolveValidPath` chain |
| `FilePane.svelte:1531` | `directory-deleted` event | `resolveValidPath(currentPath)` |
| `FilePane.svelte:1641–1652` | dir-exists poll | `pathExists` + `pathExists(volumePath)` + `resolveValidPath` chain |

### Why a web worker wouldn't help

The blocking isn't JS computation — it's IPC calls to Rust that await kernel syscalls. Web Workers can't call Tauri's
`invoke()`. The fix is timeouts on the Rust side and optimistic navigation patterns on the frontend.

## Approach

Two layers, each independently valuable:

### Layer 1: Rust-side timeout on `pathExists` (caps the worst case)

Wrap `volume.exists()` in a `tokio::time::timeout`. If the filesystem doesn't respond within a deadline, return `false`.

**File:** `apps/desktop/src-tauri/src/commands/file_system.rs`, `path_exists` function.

```rust
use tokio::time::{timeout, Duration};

const PATH_EXISTS_TIMEOUT: Duration = Duration::from_secs(2);

#[tauri::command]
pub async fn path_exists(volume_id: Option<String>, path: String) -> bool {
    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());
    let expanded_path = if volume_id == "root" { expand_tilde(&path) } else { path };

    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let path_for_check = expanded_path.clone();
        return timeout(
            PATH_EXISTS_TIMEOUT,
            tokio::task::spawn_blocking(move || volume.exists(Path::new(&path_for_check))),
        )
        .await
        .unwrap_or(Ok(false))  // Timeout → false
        .unwrap_or(false);     // JoinError → false
    }

    // Fallback for unknown volumes
    let path_buf = PathBuf::from(expanded_path);
    path_buf.exists()
}
```

This caps the worst-case `determineNavigationPath` call from 120 seconds to ~6 seconds (3 × 2s). That's still not
great, which is why we also need layer 2.

**Consideration:** The fallback `path_buf.exists()` at the bottom also blocks (it runs on the Tauri async runtime
thread, not `spawn_blocking`). Wrap it too.

**Implementation detail:** Extract the timeout + `spawn_blocking` pattern into a reusable helper:

```rust
/// Runs a blocking closure on the blocking thread pool with a timeout.
/// Returns the fallback value if the closure doesn't complete in time.
async fn blocking_with_timeout<T: Send + 'static>(
    timeout_duration: Duration,
    fallback: T,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    match timeout(timeout_duration, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => result,
        _ => fallback, // Timeout or JoinError
    }
}
```

Then `path_exists` becomes:

```rust
#[tauri::command]
pub async fn path_exists(volume_id: Option<String>, path: String) -> bool {
    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());
    let expanded_path = if volume_id == "root" { expand_tilde(&path) } else { path };

    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let path_for_check = expanded_path.clone();
        return blocking_with_timeout(PATH_EXISTS_TIMEOUT, false, move || {
            volume.exists(Path::new(&path_for_check))
        }).await;
    }

    let path_buf = PathBuf::from(expanded_path);
    blocking_with_timeout(PATH_EXISTS_TIMEOUT, false, move || path_buf.exists()).await
}
```

This helper is independently testable — see the testing section.

### Layer 2: Optimistic frontend navigation (eliminates perceived freeze)

The core idea: **update UI state immediately, navigate optimistically, and resolve the "best" path in the background.**

#### 2a. `handleVolumeChange`: show loading before resolving

Current flow:
```
await determineNavigationPath()   ← blocks, no UI feedback
setPanePath(resolved)             ← finally triggers FilePane
```

New flow:
```
setPaneVolumeId(pane, volumeId)   ← immediate
setPanePath(pane, targetPath)     ← immediate, triggers FilePane loading spinner
void resolveAndCorrect(...)       ← background, re-navigates if needed
```

```typescript
async function handleVolumeChange(pane, volumeId, volumePath, targetPath) {
    const oldPath = getPanePath(pane)
    void saveLastUsedPathForVolume(getPaneVolumeId(pane), oldPath)

    if (!volumes.find((v) => v.id === volumeId)) {
        volumes = await listVolumes()
    }

    // Immediately navigate to the target path (optimistic)
    setPaneVolumeId(pane, volumeId)
    setPanePath(pane, targetPath)
    setPaneHistory(pane, push(getPaneHistory(pane), { volumeId, path: targetPath }))
    focusedPane = pane

    // Capture a generation counter to guard against stale corrections.
    // If the user navigates again before this resolves, the generation won't match
    // and the correction is discarded.
    const generation = ++volumeChangeGeneration
    const other = otherPane(pane)
    void determineNavigationPath(volumeId, volumePath, targetPath, {
        otherPaneVolumeId: getPaneVolumeId(other),
        otherPanePath: getPanePath(other),
    }).then((betterPath) => {
        // Discard if user has navigated away (generation mismatch) or path already correct
        if (generation !== volumeChangeGeneration) return
        if (betterPath !== targetPath && betterPath !== getPanePath(pane)) {
            setPanePath(pane, betterPath)
        }
    })

    void cancelNavPriority(oldPath)
    void prioritizeDir(targetPath, 'current_dir')
    void saveAppStatus({ ... })
}
```

A module-level `let volumeChangeGeneration = 0` counter guards against stale corrections. Every call to
`handleVolumeChange` increments it; the `.then()` callback checks that its captured generation still matches. If the
user navigates away (to a third path, another volume, and so on) before the background resolution finishes, the
correction is silently discarded.

This means: the pane shows a loading spinner *instantly*, and the listing starts for `targetPath` right away. If the
listing fails (path doesn't exist), FilePane's existing error handler navigates to a valid parent. If
`determineNavigationPath` finds a "better" path and the user hasn't navigated away, it silently redirects.

#### 2b. `determineNavigationPath`: parallel checks with timeout

Change sequential `pathExists` calls to parallel with a short frontend timeout:

```typescript
export async function determineNavigationPath(
    volumeId: string, volumePath: string, targetPath: string, otherPane: OtherPaneState,
): Promise<string> {
    if (targetPath !== volumePath) return targetPath

    const timeout = <T>(promise: Promise<T>, ms: number, fallback: T): Promise<T> =>
        Promise.race([promise, new Promise<T>((r) => setTimeout(() => r(fallback), ms))])

    const [otherPaneValid, lastUsedResult] = await Promise.all([
        otherPane.otherPaneVolumeId === volumeId
            ? timeout(pathExists(otherPane.otherPanePath), 500, false)
            : Promise.resolve(false),
        getLastUsedPathForVolume(volumeId).then((p) =>
            p ? timeout(pathExists(p), 500, false).then((ok) => (ok ? p : null)) : null,
        ),
    ])

    if (otherPaneValid) return otherPane.otherPanePath
    if (lastUsedResult) return lastUsedResult
    return volumeId === DEFAULT_VOLUME_ID ? '~' : volumePath
}
```

This makes both checks parallel and caps each at 500ms. Combined with the Rust 2-second timeout, the faster one wins.

#### 2c. `resolveValidPath`: timeout per step, bail early

```typescript
export async function resolveValidPath(targetPath: string): Promise<string | null> {
    const checkWithTimeout = (p: string): Promise<boolean> =>
        Promise.race([pathExists(p), new Promise<false>((r) => setTimeout(() => r(false), 1000))])

    let path = targetPath
    while (path !== '/' && path !== '') {
        if (await checkWithTimeout(path)) return path
        path = path.substring(0, Math.max(path.lastIndexOf('/'), 1))
        if (path === '/') break
    }
    if (await checkWithTimeout('/')) return '/'
    return '~' // Last resort — home dir is almost always reachable
}
```

Key change: each step gets a 1-second timeout. On a 5-level-deep network path that's hung, the worst case goes from
5 × 60s = 300s to 5 × 1s = 5s — and with the Rust-side 2s timeout it'd be 5 × min(1s, 2s) = 5s.

#### 2d. `handleCancelLoading`: instant fallback

When the user presses ESC during a load, don't call `resolveValidPath` (which might also hang). Just go home:

```typescript
function handleCancelLoading(pane, selectName?) {
    const entry = getCurrentEntry(getPaneHistory(pane))

    if (entry.volumeId === 'network') {
        // ... existing network handling
    } else {
        // Immediately navigate to a known-safe local path
        setPanePath(pane, '~')
        setPaneVolumeId(pane, DEFAULT_VOLUME_ID)
        void saveAppStatus({ ... })
    }
    containerElement?.focus()
}
```

The user pressed ESC — they want out. Don't make them wait while we walk the parent tree of a dead network mount.

#### 2e. `handleNavigationAction`: optimistic with fallback

For back/forward navigation, navigate immediately and let the listing error handler deal with missing paths:

```typescript
async function handleNavigationAction(action: string) {
    // ... existing history logic ...

    const targetEntry = getCurrentEntry(newHistory)
    // Navigate immediately — if path is gone, FilePane's error handler resolves upward
    updatePaneAfterHistoryNavigation(pane, newHistory, targetEntry.path)
}
```

Remove the `await resolveValidPath(targetEntry.path)` gate. FilePane's listing error handler at line 936-958 already
does this: checks if the path is deleted, and if so, calls `resolveValidPath` and re-navigates. No need to do it
before the listing starts.

#### 2f. FilePane's `resolveValidPath` calls: add timeouts

The fire-and-forget `resolveValidPath` calls in FilePane (lines 943, 1531, 1652) don't block navigation but could
silently hang for minutes. Use the timeout-aware version from 2c so they resolve within seconds even on dead mounts.

## What about the dir-exists poll?

`FilePane.svelte:1630–1669` polls `pathExists(currentPath)` at intervals. When sitting on a slow network path, each
poll might hang for the full kernel timeout. The Rust-side 2-second timeout (layer 1) fixes this automatically — polls
resolve within 2 seconds even on hung mounts.

## Testing

### Automated
- **Rust unit test for `blocking_with_timeout` helper:** Test the extracted helper directly with a closure that
  `thread::sleep`s beyond the timeout — verify it returns the fallback. Also test a fast closure returns the real value.
  This is deterministic and doesn't need filesystem access.
- **TypeScript unit tests for `determineNavigationPath`:** Mock `pathExists` to simulate slow/fast responses,
  verify parallel execution and timeout behavior.
- **TypeScript unit tests for `resolveValidPath`:** Mock `pathExists` with delays, verify timeout per step.

### Manual
1. Mount a network share (SMB). Navigate to it in one pane.
2. Disconnect the network (turn off Wi-Fi or unplug Ethernet).
3. Click a local volume in the breadcrumb — should show loading spinner within 100ms, not freeze.
4. Press ESC during any slow operation — should navigate to `~` instantly.
5. Use back/forward to navigate through history that includes the dead network path — should not freeze.
6. Reconnect network — should be able to navigate back to the share normally.

## Task list

### Milestone 1: Rust-side timeout (quick win)
- [x] Extract `blocking_with_timeout` helper (timeout + `spawn_blocking` in one function)
- [x] Use the helper in `path_exists` for both the volume path and the fallback `path_buf.exists()`
- [x] Add Rust unit tests for `blocking_with_timeout`: fast closure returns value, slow closure returns fallback
- [x] Run `./scripts/check.sh --check clippy,rustfmt,rust-tests`

### Milestone 2: Optimistic volume switching
- [x] Add `volumeChangeGeneration` counter to `DualPaneExplorer.svelte`
- [x] Restructure `handleVolumeChange` to update pane state immediately, resolve "best path" in background with generation guard
- [x] Make `determineNavigationPath` use `Promise.all` with 500ms frontend timeouts per check
- [x] Make `resolveValidPath` use 1-second timeout per step
- [x] Add TypeScript unit tests for `determineNavigationPath` with timeout behavior
- [x] Add TypeScript unit tests for `resolveValidPath` with timeout behavior
- [x] Run `./scripts/check.sh --check svelte-check,desktop-svelte-eslint,svelte-tests`

### Milestone 3: Instant cancel and optimistic history navigation
- [x] Change `handleCancelLoading` to navigate to `~` immediately instead of calling `resolveValidPath`
- [x] Change `handleNavigationAction` (back/forward) to navigate immediately, removing the `resolveValidPath` gate
- [x] Verify FilePane's existing listing error handler covers the deleted-path case
- [x] Run `./scripts/check.sh --check svelte-check,desktop-svelte-eslint,svelte-tests`

### Milestone 4: Manual verification and cleanup
- [ ] Manual test: mount network share, disconnect, switch volume — spinner appears instantly
- [ ] Manual test: ESC during slow load — navigates to `~` instantly
- [ ] Manual test: back/forward through dead network path — no freeze
- [ ] Manual test: reconnect and navigate back to share — works normally
- [x] Update `navigation/CLAUDE.md` to document the timeout and optimistic patterns
- [ ] Run full `./scripts/check.sh --svelte` and `./scripts/check.sh --rust`
