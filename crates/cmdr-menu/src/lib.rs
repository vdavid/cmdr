//! Cmdr's menu bar as data, with no UI toolkit: every menu, item, check group, accelerator, and
//! label key (`bar`, in the words `spec` defines), every right-click menu (`context`), the item ids and the commands they run (`ids`),
//! which rows are enabled (`enabled`), what a click asks for (`click`), the Sort by checks
//! (`sort`), the live labels (`labels`), and the accelerator vocabulary (`accelerators`).
//!
//! Two hosts: the desktop builds native menus from it (`apps/desktop/src-tauri/src/menu/`), and
//! the browser demo draws the macOS bar through `cmdr-listing-wasm`. See `CLAUDE.md`.

pub mod accelerators;
pub mod bar;
pub mod click;
pub mod context;
pub mod enabled;
pub mod ids;
pub mod labels;
pub mod sort;
pub mod spec;
