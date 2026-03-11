# Non-reactive file data store for large directories

Loading 20k+ files into Svelte reactive state causes multi-second UI freezes due to Svelte's internal tracking of large
arrays. The solution: store file data in a plain JavaScript class (`FileDataStore`) outside of Svelte's reactivity
system, and have the virtual scroll components request only the visible range (~50-100 items) to put into reactive state.

## The problem (benchmark data, 50k files)

| Step                   | Time     |
| ---------------------- | -------- |
| Rust list_directory    | 308ms    |
| JSON serialize (Rust)  | 18ms     |
| IPC transfer (17.4 MB) | ~4,100ms |
| JSON.parse (JS)        | 67ms     |
| Svelte reactivity      | **9.5s** |
| **Total**              | **~14s** |

Even though we use virtual scrolling (only ~50-100 DOM elements), the `$derived(files)` computation still runs a filter
over the entire array, and Svelte's reactivity system still processes the full array assignment internally.

## Alternatives rejected

- **Web Worker**: `postMessage()` has serialization cost; Tauri IPC must still run on main thread; overkill.
- **Chunked loading into Svelte**: Array concatenation is O(n²); Svelte still tracks full array; total work same or worse.

## Architecture

```
FileDataStore (plain JS class, NOT reactive)
  - files: FileEntry[] (plain array, 20k+ items)
  - getRange(start, end): FileEntry[]
  - filterHidden(showHidden): void

          │ on scroll
          ▼
Svelte Component
  let visibleItems = $state<FileEntry[]>([])  // ~50-100
  On scroll: visibleItems = store.getRange(start, end)
```

Cost: Svelte reactivity O(visible items) = ~50-100 items → <5ms. Store operations: O(1) array slice → <1ms.
