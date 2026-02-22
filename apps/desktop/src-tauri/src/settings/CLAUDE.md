# Settings (Rust)

Thin read-only settings loader used during Rust startup. The frontend owns all settings via `tauri-plugin-store`; this module just reads what was persisted so the backend can configure itself at launch.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | Re-exports `load_settings` from `legacy` |
| `legacy.rs` | `Settings` struct + `load_settings`: tries `settings-v2.json` first, falls back to `settings.json`, then uses defaults |

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

## File formats

**`settings-v2.json`** (current): flat JSON with literal dot-notation string keys.

```json
{ "showHiddenFiles": true, "developer.mcpEnabled": true, "developer.mcpPort": 9222 }
```

These are top-level keys — the dot is part of the key name, not a nesting separator. `parse_settings_v2` reads them manually (serde can't express dot-notation field names as struct fields).

**`settings.json`** (legacy): nested JSON with camelCase keys, parsed via serde aliases.

## Key patterns

- **One-way read only.** This module never writes. All writes go through the frontend's settings store.
- Module is named `legacy` because ideally the frontend would push relevant values via IPC at startup rather than requiring Rust to parse the store file directly.
- `full_disk_access_choice` is marked `#[allow(dead_code)]` — it is persisted by the frontend but the backend takes no action on it.
- Falls back gracefully at every step: missing file → try next format → use `Default`.

## Dependencies

External: `tauri::Manager` (for `app_data_dir`)
Internal: none
