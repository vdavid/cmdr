# ADR 007: Use JSON for Tauri IPC

## Status

Accepted and validated by benchmarks

## Context

Tauri 2.0 supports both JSON (default) and binary formats via Raw Payloads (MessagePack, Protobuf, etc.). For lists of
50k files, we need to choose the right serialization approach.

### Options considered

1. **JSON (default)**: Simple, debuggable, no extra dependencies
2. **MessagePack**: Smaller payload, but complex to integrate with Tauri
3. **Protobuf**: Schema-based, very compact, complex setup

### Benchmarks (actual measurements, Dec 2024)

We tested JSON vs. MessagePack with real directory listings:

| Files | JSON Time  | JSON Size | MsgPack Time | MsgPack Size |
| ----- | ---------- | --------- | ------------ | ------------ |
| 5k    | **454ms**  | 1.69 MB   | 718ms        | 1.41 MB      |
| 50k   | **4782ms** | 16.99 MB  | 6432ms       | 13.78 MB     |

**Key finding: MessagePack is 34-58% SLOWER despite being 17-19% smaller.**

### Why binary formats are slower in Tauri

When returning `Vec<u8>` from a Tauri command, Tauri serializes it as a **JSON array of numbers**:

```
[82, 117, 115, 116, 121, ...]  // Each byte becomes 1-3 chars + comma
```

This means:

1. Binary data is wrapped in JSON anyway (negating size benefits)
2. JSON parsing is still required
3. Then binary decoding adds more overhead

## Decision

Use **JSON** for all Tauri IPC.

Rationale:

1. JSON is measurably faster than MessagePack/Protobuf through Tauri's invoke system
2. JSON is simpler to debug (readable in browser devtools)
3. No additional dependencies needed
4. Native `JSON.parse()` in JavaScript is heavily optimized

## Consequences

### Positive

- Best actual performance (benchmarked, not theoretical)
- Simpler implementation
- Easier debugging with browser devtools
- Tauri's default path, best documented

### Negative

- Larger payloads than theoretical binary formats
- For very large directories (50k+), IPC becomes the bottleneck

## Notes

### Alternative approaches to speed up large directory transfers

If IPC becomes a bottleneck for very large directories (100k+), consider:

1. **Chunked IPC** - Split into multiple 10k-item requests, process progressively
2. **WebSocket sidecar** - Separate WebSocket server for raw binary transfer
3. **Tauri Events with raw payloads** - Events can carry binary data differently than invoke
4. **Virtual scrolling + lazy loading** - Only fetch visible items, load more on scroll
5. **Reduce payload size** - Send fewer fields initially (name, type only), lazy-load metadata

Current recommendation: Focus on virtual scrolling and chunked loading rather than binary formats.
