use super::NativeEditSelector;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn(&tauri::AppHandle<tauri::Wry>, NativeEditSelector) = run::<tauri::Wry>;

/// Runs `selector` in the focused window's focused field.
pub(super) use imp::run;
