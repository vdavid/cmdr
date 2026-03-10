# Settings (Rust)

Thin read-only settings loader used during Rust startup. The frontend owns all settings via `tauri-plugin-store`; this module just reads what was persisted so the backend can configure itself at launch.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | Re-exports `load_settings` from `legacy` |
| `legacy.rs` | `Settings` struct + `load_settings`: reads `settings.json`, falls back to defaults |

## Settings struct

```rust
Settings {
    show_hidden_files: bool,           // default true
    full_disk_access_choice: ...,      // persisted by frontend only, #[allow(dead_code)]
    developer_mcp_enabled: Option<bool>,
    developer_mcp_port: Option<u16>,
    indexing_enabled: Option<bool>,
}
```

## File format

**`settings.json`**: flat JSON with literal dot-notation string keys, written by `tauri-plugin-store`.

```json
{ "showHiddenFiles": true, "developer.mcpEnabled": true, "developer.mcpPort": 9224 }
```

These are top-level keys — the dot is part of the key name, not a nesting separator. `parse_settings` reads them manually (serde can't express dot-notation field names as struct fields).

## Key decisions

**Decision**: Rust reads the settings file directly instead of receiving values via IPC from the frontend.
**Why**: Several backend systems (MCP server, hidden files filter, indexing) need their config *before* any frontend window loads. Waiting for the frontend to boot and push settings would create a race condition or require delaying backend initialization. Reading the file directly means the backend is configured immediately at launch.

**Decision**: Manual JSON field extraction in `parse_settings` instead of serde auto-derivation.
**Why**: `tauri-plugin-store` writes flat JSON with literal dot-notation string keys like `"developer.mcpEnabled"`. Serde's `rename` attribute can handle this per-field, but the dot is part of the key name, not a nesting separator. The manual extraction makes this non-obvious format explicit and avoids confusion about nested vs. flat structure.

## Key patterns

- **One-way read only.** This module never writes. All writes go through the frontend's settings store.
- Module is named `legacy` because ideally the frontend would push relevant values via IPC at startup rather than requiring Rust to parse the store file directly.
- `full_disk_access_choice` is marked `#[allow(dead_code)]` — it is persisted by the frontend but the backend takes no action on it.
- Falls back gracefully: missing file → use `Default`.

## Dependencies

External: none
Internal: `crate::config::resolved_app_data_dir` (for app data directory with dev isolation)
