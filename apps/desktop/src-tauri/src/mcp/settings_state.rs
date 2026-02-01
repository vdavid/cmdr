//! Settings state storage for MCP settings tools.
//!
//! Stores the current settings state so MCP tools can access it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::RwLock;
use tauri::AppHandle;
use tauri::Manager;

/// Represents a setting item with its definition and current value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingItem {
    pub id: String,
    pub label: String,
    pub description: String,
    pub setting_type: String,
    pub value: Value,
    pub default_value: Value,
    pub is_modified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Value>,
}

/// Represents a section in the settings sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSection {
    pub name: String,
    pub path: Vec<String>,
    pub subsections: Vec<SettingsSection>,
}

/// Represents a command with its shortcuts for the keyboard shortcuts section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCommand {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub shortcuts: Vec<String>,
    pub default_shortcuts: Vec<String>,
    pub is_modified: bool,
}

/// Settings state stored by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SettingsState {
    /// Whether the settings window is open
    pub is_open: bool,
    /// Currently selected section path
    pub selected_section: Vec<String>,
    /// All available sections
    pub sections: Vec<SettingsSection>,
    /// Settings in the current section
    pub current_settings: Vec<SettingItem>,
    /// All shortcut commands (for keyboard shortcuts section)
    pub shortcuts: Vec<ShortcutCommand>,
}

/// Shared state for settings.
#[derive(Debug, Default)]
pub struct SettingsStateStore {
    pub state: RwLock<SettingsState>,
}

impl SettingsStateStore {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(SettingsState::default()),
        }
    }

    pub fn get_state(&self) -> SettingsState {
        self.state.read().unwrap().clone()
    }

    pub fn set_state(&self, state: SettingsState) {
        *self.state.write().unwrap() = state;
    }

    pub fn set_is_open(&self, is_open: bool) {
        self.state.write().unwrap().is_open = is_open;
    }

    pub fn set_selected_section(&self, section: Vec<String>) {
        self.state.write().unwrap().selected_section = section;
    }

    pub fn set_sections(&self, sections: Vec<SettingsSection>) {
        self.state.write().unwrap().sections = sections;
    }

    pub fn set_current_settings(&self, settings: Vec<SettingItem>) {
        self.state.write().unwrap().current_settings = settings;
    }

    pub fn set_shortcuts(&self, shortcuts: Vec<ShortcutCommand>) {
        self.state.write().unwrap().shortcuts = shortcuts;
    }
}

/// Tauri command to update settings state from frontend.
#[tauri::command]
pub fn mcp_update_settings_state(app: AppHandle, state: SettingsState) {
    if let Some(store) = app.try_state::<SettingsStateStore>() {
        store.set_state(state);
    }
}

/// Tauri command to update settings window open state.
#[tauri::command]
pub fn mcp_update_settings_open(app: AppHandle, is_open: bool) {
    if let Some(store) = app.try_state::<SettingsStateStore>() {
        store.set_is_open(is_open);
    }
}

/// Tauri command to update selected section.
#[tauri::command]
pub fn mcp_update_settings_section(app: AppHandle, section: Vec<String>) {
    if let Some(store) = app.try_state::<SettingsStateStore>() {
        store.set_selected_section(section);
    }
}

/// Tauri command to update available sections.
#[tauri::command]
pub fn mcp_update_settings_sections(app: AppHandle, sections: Vec<SettingsSection>) {
    if let Some(store) = app.try_state::<SettingsStateStore>() {
        store.set_sections(sections);
    }
}

/// Tauri command to update current settings in the selected section.
#[tauri::command]
pub fn mcp_update_current_settings(app: AppHandle, settings: Vec<SettingItem>) {
    if let Some(store) = app.try_state::<SettingsStateStore>() {
        store.set_current_settings(settings);
    }
}

/// Tauri command to update shortcuts list.
#[tauri::command]
pub fn mcp_update_shortcuts(app: AppHandle, shortcuts: Vec<ShortcutCommand>) {
    if let Some(store) = app.try_state::<SettingsStateStore>() {
        store.set_shortcuts(shortcuts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_state_store() {
        let store = SettingsStateStore::new();

        let state = SettingsState {
            is_open: true,
            selected_section: vec!["General".to_string(), "Appearance".to_string()],
            sections: vec![SettingsSection {
                name: "General".to_string(),
                path: vec!["General".to_string()],
                subsections: vec![],
            }],
            current_settings: vec![],
            shortcuts: vec![],
        };

        store.set_state(state);
        let retrieved = store.get_state();

        assert!(retrieved.is_open);
        assert_eq!(retrieved.selected_section, vec!["General", "Appearance"]);
        assert_eq!(retrieved.sections.len(), 1);
    }

    #[test]
    fn test_set_is_open() {
        let store = SettingsStateStore::new();

        assert!(!store.get_state().is_open);

        store.set_is_open(true);
        assert!(store.get_state().is_open);
    }
}
