//! A web or mail link clicked in a rendered document. Opened by the host (the user's
//! browser or mail app), so the viewer's webview needs no opener grant and only `http`,
//! `https`, and `mailto` ever leave the viewer (`markdown_links.rs` decides which).

use super::super::ViewerError;

/// Opens an external link.
pub trait LinkOpener: Send + Sync {
    /// Opens `url`, already checked to be `http`, `https`, or `mailto`.
    fn open_url(&self, url: &str) -> Result<(), ViewerError>;
}

/// Opens nothing, and says nothing went wrong: a host with nowhere to send a link.
pub struct NoOpener;

impl LinkOpener for NoOpener {
    fn open_url(&self, _url: &str) -> Result<(), ViewerError> {
        Ok(())
    }
}
