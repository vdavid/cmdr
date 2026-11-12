use super::MnemonicModifier;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn(&tauri::Window, MnemonicModifier) -> tauri::Result<()> = apply;

/// Makes `window`'s bar mnemonics answer to `modifier`. A no-op where the bar has none.
pub(super) use imp::apply;
