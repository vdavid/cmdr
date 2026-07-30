//! The app's frontend event surface for subsystems that don't speak Tauri.
//!
//! A subsystem describes what happened as a typed value; a module here turns it
//! into the Tauri payload the frontend subscribes to. That keeps wire names,
//! payload shapes, and the words a human reads on this side of the boundary.
//!
//! - `index_mapping`: the drive index, media index, and importance subsystems.

pub mod index_mapping;
