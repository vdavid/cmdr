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

## Key decisions

**Decision**: Rust reads the settings file directly instead of receiving values via IPC from the frontend.
**Why**: Several backend systems (MCP server, hidden files filter, indexing) need their config *before* any frontend window loads. Waiting for the frontend to boot and push settings would create a race condition or require delaying backend initialization. Reading the file directly means the backend is configured immediately at launch.

**Decision**: Manual JSON field extraction in `parse_settings_v2` instead of serde auto-derivation.
**Why**: `tauri-plugin-store` writes flat JSON with literal dot-notation string keys like `"developer.mcpEnabled"`. Serde's `rename` attribute can handle this per-field, but the dot is part of the key name, not a nesting separator. The manual extraction makes this non-obvious format explicit and avoids confusion about nested vs. flat structure.

**Decision**: Try `settings-v2.json` first, then fall back to `settings.json`, then defaults.
**Why**: The app migrated from a nested JSON format (v1) to the flat dot-notation format (v2) written by `tauri-plugin-store`. Users upgrading from older versions still have the v1 file. The cascade ensures settings survive across app updates without requiring an explicit migration step.

## Key patterns

- **One-way read only.** This module never writes. All writes go through the frontend's settings store.
- Module is named `legacy` because ideally the frontend would push relevant values via IPC at startup rather than requiring Rust to parse the store file directly.
- `full_disk_access_choice` is marked `#[allow(dead_code)]` — it is persisted by the frontend but the backend takes no action on it.
- Falls back gracefully at every step: missing file → try next format → use `Default`.

## Dependencies

External: `tauri::Manager` (for `app_data_dir`)
Internal: none
