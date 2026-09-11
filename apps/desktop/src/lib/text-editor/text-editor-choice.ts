/**
 * The stored `behavior.textEditorApp` value that means "the macOS plain-text
 * default" (`open -t`). Mirrors `SYSTEM_DEFAULT_CHOICE` in
 * `src-tauri/src/file_system/text_editor.rs`.
 *
 * ❗ A leaf with zero imports, so `settings/sections/` can name it without an
 * import cycle through `$lib/settings`.
 */
export const SYSTEM_DEFAULT_EDITOR_CHOICE = 'system'
