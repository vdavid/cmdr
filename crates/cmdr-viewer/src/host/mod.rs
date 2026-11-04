//! What the viewer asks of whoever hosts it: the seams, one trait per concern.
//!
//! The viewer core never reaches the app's registries, its event bus, its temp dirs, or
//! the network: it asks here. The desktop app answers through `file_viewer/app_host.rs`;
//! a host with none of those (a test, the browser demo) answers with the `No*` defaults,
//! which do nothing and fail nothing. Modeled on `crates/cmdr-listing/src/host/`.
//!
//! - [`volumes`]: which volume serves an id, and whether a path routes over its parent.
//! - [`pulls`]: a bounded temp copy of a file the OS can't open.
//! - [`images`]: where a rendered Markdown document's pictures come from.
//! - [`events`]: what the viewer tells the window (a watched file changed, a token dropped).
//! - [`analytics`]: the `viewer_opened` facts.
//! - [`opener`]: a web or mail link clicked in a document.
//!
//! The debug log's row transform is pure, so it's a plain function on [`ViewerHost`].

pub mod analytics;
pub mod events;
pub mod images;
pub mod opener;
pub mod pulls;
pub mod volumes;

use std::borrow::Cow;
use std::sync::Arc;

/// How the debug log's view shows one row (`RowText::LogDisplay`): log markers hidden,
/// each marked value as itself.
pub type LogDisplay = for<'a> fn(&'a str) -> Cow<'a, str>;

/// Every seam the viewer reaches its host through.
#[derive(Clone)]
pub struct ViewerHost {
    pub volumes: Arc<dyn volumes::ViewerVolumes>,
    pub pulls: Arc<dyn pulls::ViewerPulls>,
    pub images: Arc<dyn images::DocumentImages>,
    pub events: Arc<dyn events::ViewerEvents>,
    pub analytics: Arc<dyn analytics::ViewerAnalytics>,
    pub opener: Arc<dyn opener::LinkOpener>,
    pub log_display: LogDisplay,
}

impl ViewerHost {
    /// A host that answers nothing: no volumes, no pulls, no document images, events and
    /// analytics go nowhere, links don't open, and the log view shows rows as they are.
    pub fn detached() -> Self {
        Self {
            volumes: Arc::new(volumes::NoVolumes),
            pulls: Arc::new(pulls::NoPulls),
            images: Arc::new(images::NoImages),
            events: Arc::new(events::NoEvents),
            analytics: Arc::new(analytics::NoAnalytics),
            opener: Arc::new(opener::NoOpener),
            log_display: as_stored,
        }
    }
}

/// A row as the file stores it.
pub(crate) fn as_stored(line: &str) -> Cow<'_, str> {
    Cow::Borrowed(line)
}
