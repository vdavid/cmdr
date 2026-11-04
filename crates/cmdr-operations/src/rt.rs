//! Where the write engine reads its clocks, waits, and yields: `cmdr_fs::rt`,
//! named here so engine code says `crate::rt::…`. Natively it's tokio and `std`
//! under other names (the desktop runs the same code); in the browser build it's
//! the page's event loop and clock. `crates/cmdr-fs/DETAILS.md` § "The browser
//! build". `clippy.toml` refuses the direct calls.
//!
//! ❗ Two clocks, never conflated: `now` / `since` / `Instant` are `std`'s (a
//! real clock natively), `time::{now, Instant, sleep, …}` are tokio's (which a
//! `start_paused` test controls). Swapping one for the other moves a site on or
//! off the paused test clock.

pub(crate) use cmdr_fs::rt::{Instant, now, since, task, time, wall_now};
