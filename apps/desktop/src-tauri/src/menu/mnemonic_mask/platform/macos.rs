use super::MnemonicModifier;

/// macOS menus have no mnemonics, so there's nothing to suspend.
pub(crate) fn apply(_window: &tauri::Window, _modifier: MnemonicModifier) -> tauri::Result<()> {
    Ok(())
}
