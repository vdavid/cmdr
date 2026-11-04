//! Where the operation log reads its clocks and waits: `cmdr_fs::rt`, named here
//! so log code says `crate::rt::…`. Natively it's tokio and `std` under other
//! names; in the browser build, the page's event loop and clock.
//! `crates/cmdr-fs/DETAILS.md` § "The browser build". `clippy.toml` refuses the
//! direct calls.

pub(crate) use cmdr_fs::rt::{time, wall_now};
