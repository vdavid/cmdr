# JSON vs binary formats for Tauri IPC

Tauri 2.0 supports both JSON (default) and binary formats via Raw Payloads (MessagePack, Protobuf, etc.). We benchmarked
JSON vs. MessagePack with real directory listings (Dec 2024):

| Files | JSON time  | JSON size | MsgPack time | MsgPack size |
| ----- | ---------- | --------- | ------------ | ------------ |
| 5k    | **454ms**  | 1.69 MB   | 718ms        | 1.41 MB      |
| 50k   | **4782ms** | 16.99 MB  | 6432ms       | 13.78 MB     |

**Key finding: MessagePack is 34-58% SLOWER despite being 17-19% smaller.**

## Why binary formats are slower in Tauri

When returning `Vec<u8>` from a Tauri command, Tauri serializes it as a JSON array of numbers:
`[82, 117, 115, 116, 121, ...]` — each byte becomes 1-3 chars + comma.

This means: binary data is wrapped in JSON anyway (negating size benefits), JSON parsing is still required, and then
binary decoding adds more overhead.

## If IPC becomes a bottleneck for 100k+ directories

Consider: chunked IPC (multiple 10k-item requests), WebSocket sidecar for raw binary transfer, Tauri Events with raw
payloads (events carry binary data differently than invoke), virtual scrolling + lazy loading, or reducing payload size
(fewer fields initially).
