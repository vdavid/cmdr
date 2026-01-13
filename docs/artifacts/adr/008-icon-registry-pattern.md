# ADR 008: Icon registry pattern for efficient icon transmission

## Status

Accepted

## Context

File icons need to be displayed for every file in the list. Without optimization, 50k JPEG files would transmit 50k
identical JPEG icons (~2–4KB each = 100–200MB of redundant data).

### Options considered

1. **Inline icons**: Send icon data with each file entry
2. **Icon IDs**: Send icon reference, fetch icons separately
3. **CSS sprites**: Prebuilt sprite sheet of common icons
4. **No icons**: Use emoji or text-only display

## Decision

Use an **icon registry pattern**:

1. Backend generates stable `iconId` for each file:
    - Extension-based: `"ext:jpg"`, `"ext:pdf"` (99% of files)
    - Custom icons: `"custom:<hash>"` (apps, special folders)

2. `FileEntry` includes only `iconId`, not icon data

3. Separate `get_icons(icon_ids: Vec<String>)` command returns icon data

4. Frontend caches icons in localStorage/IndexedDB

### Icon format

- Size: 32×32 pixels (good for retina at 2x)
- Format: WebP (~50% smaller than PNG)
- Fallback: Extension-based generic icons

### Data flow

```
┌─────────────┐     ┌───────────────────┐     ┌─────────────┐
│  File list  │────▶│  iconId refs      │────▶│  Frontend   │
│  response   │     │  ("ext:jpg", ...) │     │  checks     │
└─────────────┘     └───────────────────┘     │  cache      │
                                              └──────┬──────┘
                                                     │
                             ┌───────────────────────┘
                             ▼
                  ┌─────────────────────┐
                  │  get_icons() for    │
                  │  uncached IDs only  │
                  └─────────────────────┘
```

## Consequences

### Positive

- 50k files transmit ~50 unique icon IDs (not 50k icon blobs)
- Icons cached persistently across sessions
- Lazy loading — only fetch icons as needed
- Extension-based IDs are stable and predictable

### Negative

- Two-phase loading (file list, then icons)
- Requires frontend cache management
- Custom icons (apps, branded folders) need hash-based invalidation

### Implementation notes

- Use `file_icon_provider` crate for cross-platform icon retrieval
- Icon generation is async (~50–100 μs per unique icon via NSWorkspace)
- Backend should cache generated icons in memory during a session
