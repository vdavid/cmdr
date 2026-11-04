//! The engine's executor and clock: `cmdr-fs`'s shared seam (`cmdr_fs::rt`),
//! named here so engine code says `crate::rt::…`. Natively it's tokio and `std`
//! under their own names; the browser build swaps it. `DETAILS.md` § "The browser
//! build".

pub(crate) use cmdr_fs::rt::{Instant, Runtime, now, since, time};

/// Where an engine nobody handed a runtime spawns: `cmdr-fs`'s shared fallback
/// natively, the page's event loop in the browser.
pub(crate) fn default_runtime() -> Runtime {
    cmdr_fs::volume::host::VolumeHost::detached().runtime()
}
