use super::MnemonicModifier;

/// The Windows bar has no access keys until its menu model is decided, so there's nothing to
/// suspend yet. Once it has them, whether a page's `preventDefault` stops a Win32 access key
/// decides what goes here.
pub(crate) fn apply(_window: &tauri::Window, _modifier: MnemonicModifier) -> tauri::Result<()> {
    Ok(())
}
