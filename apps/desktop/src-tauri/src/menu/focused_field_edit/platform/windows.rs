use tauri::{AppHandle, Runtime};

use super::NativeEditSelector;

/// Whether a Win32 menu accelerator fires before WebView2 at all waits on task 63's menu model, so
/// nothing forwards yet.
pub(crate) fn run<R: Runtime>(_app: &AppHandle<R>, _selector: NativeEditSelector) {}
