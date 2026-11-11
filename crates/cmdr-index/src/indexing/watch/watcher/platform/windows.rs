//! The `notify` backend, shared with the other OS that has no event journal.

pub(crate) use super::notify_backend::{SUPPORTS_EVENT_REPLAY, start, start_branches};
