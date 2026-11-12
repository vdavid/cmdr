//! Whether this OS reads a folder's name as a package (`Cmdr.app` shown as one opaque item),
//! per OS. Only macOS does; elsewhere a folder named `X.app` is a folder.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_family = "wasm", path = "wasm.rs")]
mod imp;

const _: bool = FOLDER_NAMES_MAKE_PACKAGES;

/// Whether a folder named with a [`super::PACKAGE_EXTENSIONS`] suffix is a package here.
pub(super) use imp::FOLDER_NAMES_MAKE_PACKAGES;
