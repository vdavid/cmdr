# ADR 009: Non-reactive file data store for large directories

## Status

Accepted

## Summary

Loading 20k+ files into Svelte reactive state causes 9+ second UI freezes due to Svelte's internal tracking of large
arrays. We will store file data in a plain JavaScript class (`FileDataStore`) outside of Svelte's reactivity system, and
have the virtual scroll components request only the visible range (~50-100 items) to put into reactive state. This
reduces reactivity cost from O(total files) to O(visible files), eliminating UI freezes.

## Context, problem, solution

### Context

The file explorer needs to handle directories with 20,000–100,000 files without freezing the UI. We already have:

- Virtual scrolling that only renders ~50-100 DOM elements
- A `filesVersion` counter pattern to manually trigger Svelte reactivity
- The `allFilesRaw` array stored as plain JS (not `$state`)

Despite these optimizations, navigating into large directories still causes multi-second UI freezes.

### Problem

**Root cause:** Even though we store files in a plain JS array and only render visible items, the `$derived(files)`
computation still runs a filter over the entire array (for hidden files), and Svelte's reactivity system still processes
the full array assignment internally.

**Benchmark data (50k files):**

| Step                   | Time     |
| ---------------------- | -------- |
| Rust list_directory    | 308ms    |
| JSON serialize (Rust)  | 18ms     |
| IPC transfer (17.4 MB) | ~4,100ms |
| JSON.parse (JS)        | 67ms     |
| Svelte reactivity      | **9.5s** |
| **Total**              | **~14s** |

The Svelte reactivity step alone takes 9.5 seconds for 50k files—this is the freeze the user experiences.

**Non-goals:**

- Optimizing IPC transfer speed (tracked separately)
- Reducing JSON payload size (tracked separately)

### Possible solutions considered

**Option 2: Web Worker for file storage**

Store all file data in a Web Worker and communicate via `postMessage()`.

- Pros: `JSON.parse()` and width calculation off main thread; main thread stays free
- Cons: `postMessage()` has serialization cost; Tauri IPC must still run on main thread; adds architectural complexity
- Verdict: Overkill; doesn't address the core issue since IPC is still on main thread

**Option 3: Chunked loading into Svelte**

Load files into Svelte state in small chunks (500 at a time) to spread out the reactivity cost.

- Pros: Might feel more responsive
- Cons: Array concatenation is O(n²) if creating new arrays; Svelte still tracks full array; total work is the same or
  worse
- Verdict: Doesn't solve the fundamental problem; may actually be worse UX

### Solution

**Option 1: Non-reactive FileDataStore + visible-range requests**

Create a plain JavaScript class that holds all file data outside of Svelte's reactivity system. Virtual scroll
components request only the visible range of items to put into a small reactive array.

**Architecture:**

```
┌─────────────────────────────────────────────────────────┐
│  FileDataStore (plain JS class, NOT reactive)           │
├─────────────────────────────────────────────────────────┤
│  - files: FileEntry[] (plain array, 20k+ items)         │
│  - totalCount: number                                   │
│  - maxFilenameWidth: number (for Brief mode scrollbar)  │
│  - getRange(start, end): FileEntry[]                    │
│  - filterHidden(showHidden): void (updates internal)    │
│  - onUpdate callback (for file watcher events)          │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼ on scroll
┌─────────────────────────────────────────────────────────┐
│  Svelte Component                                       │
├─────────────────────────────────────────────────────────┤
│  let visibleItems = $state<FileEntry[]>([])  // ~50-100 │
│  let totalCount = $state(0)                             │
│  let maxWidth = $state(0)                               │
│                                                         │
│  On scroll: visibleItems = store.getRange(start, end)   │
└─────────────────────────────────────────────────────────┘
```

**Cost analysis:**

- Svelte reactivity: O(visible items) = ~50-100 items → <5ms
- Store operations: O(1) array slice → <1ms
- No Svelte tracking of 50k items → no freeze

**Brief mode width calculation:**

Use `canvas.measureText()` which is synchronous but fast (~1μs per call):

```js
const ctx = canvas.getContext('2d')
ctx.font = '14px "YourFont"'
let maxWidth = 0
for (const file of files) {
    maxWidth = Math.max(maxWidth, ctx.measureText(file.name).width)
}
// 50k measurements ≈ 50ms - acceptable
```

## Consequences

### Positive

- UI stays responsive when loading 20k+ file directories
- Scroll performance is O(visible items), not O(total items)
- Clear separation of concerns: data storage vs. UI rendering
- Easier to add features like sorting, filtering without reactivity overhead

### Negative

- More complex architecture: components must explicitly request data instead of just reading reactive state
- Need to manually handle update notifications when file watcher detects changes
- Hidden files filter logic moves from Svelte derived to imperative store method

### Notes

- The `filesVersion` pattern in the current implementation was a step toward this but didn't go far enough
- Commander One likely uses a similar pattern to achieve their ~3s load time for 50k files
